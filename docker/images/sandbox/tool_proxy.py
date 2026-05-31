#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
"""
tool_proxy.py - Generic HTTP proxy for sandbox command execution

This is a transparent execution proxy that:
1. Receives commands via HTTP from the DSB server
2. Dynamically loads and executes the requested tool
3. Returns results as JSON

This design is generic - no tool names are hardcoded. New tools work automatically.

Browser Integration:
- Uses Playwright Python for browser automation
- Connects to CDP browser at http://localhost:9222
- Maintains persistent browser/page state across requests
- Direct function calls (no subprocess overhead)
"""

import asyncio
import importlib.util
import json
import os
import sys
import traceback
from contextlib import asynccontextmanager
from typing import Any, Dict, Optional

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel

# ============================================================================
# FastAPI Application
# ============================================================================

@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan handler."""
    yield


app = FastAPI(
    title="DSB Sandbox Tool Proxy",
    description="Generic command execution proxy for sandboxes",
    version="2.0.0",
    lifespan=lifespan
)


# ============================================================================
# Generic Execution Endpoint
# ============================================================================

class ExecuteRequest(BaseModel):
    """Generic execution request - works with ANY tool."""
    interpreter: str  # "python", "python3", "node", "bash", etc.
    script_path: str  # Path to script (e.g., "/opt/tools/web_tools.py")
    action: str       # Action to perform (e.g., "web_scrape")
    args: Dict[str, Any] = {}  # Arguments for the action
    timeout: Optional[int] = 60
    environment: Optional[Dict[str, str]] = None  # Environment variables for this execution


def load_tool_module(script_path: str):
    """Dynamically load a tool module from script path."""
    if not os.path.exists(script_path):
        raise HTTPException(status_code=404, detail=f"Script not found: {script_path}")

    # Generate a unique module name based on path to avoid conflicts
    module_name = f"tool_module_{os.path.basename(script_path).split('.')[0]}"
    
    # Use importlib directly, Python caches in sys.modules automatically
    spec = importlib.util.spec_from_file_location(module_name, script_path)
    if not spec or not spec.loader:
        raise HTTPException(status_code=500, detail=f"Cannot load module: {script_path}")

    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)

    return module


async def execute_python_tool(script_path: str, action: str, args: Dict[str, Any], environment: Optional[Dict[str, str]] = None) -> Any:
    """Execute a Python tool action and return result directly."""
    from error_handler import SandboxError

    # Apply environment variables if provided
    original_env = {}
    if environment:
        for key, value in environment.items():
            original_env[key] = os.environ.get(key)
            os.environ[key] = value

    try:
        module = load_tool_module(script_path)

        # Check if module has a COMMANDS dictionary for action mapping
        commands_dict = getattr(module, "COMMANDS", None)
        if commands_dict and action in commands_dict:
            func = commands_dict[action]
        else:
            # Fall back to direct function lookup
            func = getattr(module, action, None)

        if not func or not callable(func):
            raise HTTPException(status_code=404, detail=f"Action '{action}' not found in {script_path}")

        # Regular Python function
        if asyncio.iscoroutinefunction(func):
            result = await func(args)
        else:
            result = func(args)

        return result  # FastAPI will serialize to JSON

    except SandboxError as e:
        # Return RFC 9457 Problem Details format with unified error codes
        return JSONResponse(
            status_code=e.status_code,
            content={
                "type": f"https://docs.dsb.dev/errors/{e.error_code}",
                "title": e.error_code.replace("_", " ").title(),
                "status": e.status_code,
                "detail": e.message,
                "error_code": e.error_code,
                "timestamp": "",  # Will be set by server if needed
            }
        )
    except HTTPException:
        raise
    except Exception as e:
        traceback.print_exc()
        return JSONResponse(
            status_code=500,
            content={
                "error_code": "INTERNAL_ERROR",
                "message": f"Tool execution failed: {str(e)}",
                "detail": f"Tool execution failed: {str(e)}",
            }
        )
    finally:
        # Restore original environment variables
        if environment:
            for key, value in original_env.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value


async def execute_node_tool(script_path: str, action: str, args: Dict[str, Any]) -> Dict[str, Any]:
    """Execute a Node.js tool action via subprocess (deprecated, use Python)."""
    result = await execute_subprocess("node", [script_path, action], args)
    return result


async def execute_subprocess(interpreter: str, args: list, input_data: Dict[str, Any]) -> Dict[str, Any]:
    """Execute a tool via subprocess with JSON communication."""
    import subprocess
    import tempfile

    # Write args to temp file to avoid command line escaping issues
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
        json.dump(input_data, f)
        temp_path = f.name

    try:
        # Build command
        cmd = [interpreter] + args

        # Execute with stdin from temp file
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            preexec_fn=os.setsid,
        )

        try:
            stdout, stderr = await proc.communicate(json.dumps(input_data).encode())
        except asyncio.CancelledError:
            # When the wait_for wrapper times out, the task is cancelled
            try:
                os.killpg(os.getpgid(proc.pid), 9)
            except ProcessLookupError:
                pass
            raise

        if proc.returncode != 0:
            error_msg = stderr.decode() if stderr else "Unknown error"
            raise HTTPException(status_code=500, detail=f"Command failed: {error_msg}")

        # Try to parse output as JSON
        try:
            output = stdout.decode()
            # Find JSON in output (handle mixed output)
            lines = output.strip().split('\n')
            for line in reversed(lines):
                line = line.strip()
                if line and (line.startswith('{') or line.startswith('[')):
                    return json.loads(line)
            # If no JSON found, return raw output
            return {"output": output}
        except json.JSONDecodeError:
            return {"output": stdout.decode()}

    finally:
        os.unlink(temp_path)


def _clamp_tool_timeout_seconds(raw: Optional[int]) -> float:
    """Bounds for per-request tool execution (DSB already caps upstream)."""
    max_s = int(os.getenv("TOOL_EXEC_TIMEOUT_MAX_SECS", "900"))
    if raw is None:
        return 60.0
    return float(min(max(int(raw), 1), max_s))


@app.post("/exec")
async def execute(request: ExecuteRequest):
    """
    Generic execution endpoint - works with ANY tool.

    This is the only endpoint needed. It dynamically loads and executes
    the requested tool without any hardcoded tool names.

    Browser tools are executed via direct Python function calls (no subprocess).
    Legacy Node.js tools use subprocess (will be phased out).
    """
    tool_timeout_s = _clamp_tool_timeout_seconds(request.timeout)
    try:
        if request.interpreter in ("python", "python3"):
            result = await asyncio.wait_for(
                execute_python_tool(
                    request.script_path,
                    request.action,
                    request.args,
                    request.environment,
                ),
                timeout=tool_timeout_s,
            )
            return result
        elif request.interpreter == "node":
            result = await asyncio.wait_for(
                execute_node_tool(
                    request.script_path,
                    request.action,
                    request.args,
                ),
                timeout=tool_timeout_s,
            )
            return result
        else:
            result = await asyncio.wait_for(
                execute_subprocess(
                    request.interpreter,
                    [request.script_path, request.action],
                    request.args,
                ),
                timeout=tool_timeout_s,
            )
            return result

    except asyncio.TimeoutError:
        raise HTTPException(
            status_code=504,
            detail=f"Tool execution exceeded timeout of {tool_timeout_s}s "
            f"(interpreter={request.interpreter!r}, action={request.action!r})",
        )
    except HTTPException:
        raise
    except Exception as e:
        traceback.print_exc()
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "status": "healthy",
        "service": "tool_proxy",
    }


# ============================================================================
# Main entry point
# ============================================================================

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8080)
