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

# Environment configuration
DISPLAY = os.getenv("DISPLAY", ":1")
CDP_URL = os.getenv("CDP_URL", "http://localhost:9222")

# Browser initialization timeout (ms)
BROWSER_CONNECT_TIMEOUT = int(os.getenv("BROWSER_CONNECT_TIMEOUT", "30000"))  # 30 seconds default
BROWSER_CONNECT_MAX_RETRIES = int(os.getenv("BROWSER_CONNECT_MAX_RETRIES", "10"))  # 10 retries default

# ============================================================================
# Browser State Management (Persistent across requests)
# ============================================================================

_playwright = None
_browser = None
_page = None
_lock = asyncio.Lock()
_browser_initialized = False


async def get_playwright():
    """Get or create Playwright instance."""
    global _playwright
    if _playwright is None:
        from playwright.async_api import async_playwright
        playwright_cm = async_playwright()
        # __aenter__() returns the actual Playwright object, not the context manager
        _playwright = await playwright_cm.__aenter__()
    return _playwright


async def get_browser():
    """Get or create persistent browser instance using Playwright."""
    global _browser, _page, _browser_initialized

    async with _lock:
        if _browser is None:
            playwright = await get_playwright()

            # Retry logic for CDP connection
            retry_count = 0
            last_error = None

            while retry_count < BROWSER_CONNECT_MAX_RETRIES:
                try:
                    # Try to connect to existing browser via CDP
                    print(f"[DEBUG] Connecting to CDP browser at {CDP_URL} (attempt {retry_count + 1}/{BROWSER_CONNECT_MAX_RETRIES})", flush=True)
                    _browser = await playwright.chromium.connect_over_cdp(
                        CDP_URL,
                        timeout=BROWSER_CONNECT_TIMEOUT
                    )
                    print(f"[INFO] Connected to CDP browser successfully", flush=True)

                    # Get existing page from CDP browser
                    contexts = _browser.contexts
                    if contexts and contexts[0].pages:
                        _page = contexts[0].pages[0]
                    else:
                        _page = await contexts[0].new_page()

                    # Wait for page to be ready
                    try:
                        # Try to evaluate JavaScript to verify page is ready
                        await _page.evaluate("() => true")
                        print(f"[INFO] Browser page is ready", flush=True)
                        _browser_initialized = True
                    except Exception as page_error:
                        print(f"[WARN] Page connected but not ready yet: {page_error}", flush=True)
                        # Page exists but isn't ready - this is OK, individual tools will retry
                        _browser_initialized = True

                    break  # Success, exit retry loop

                except Exception as cdp_error:
                    last_error = cdp_error
                    retry_count += 1
                    print(f"[WARN] Failed to connect to CDP (attempt {retry_count}/{BROWSER_CONNECT_MAX_RETRIES}): {cdp_error}", flush=True)

                    if retry_count < BROWSER_CONNECT_MAX_RETRIES:
                        # Exponential backoff: 1s, 2s, 4s, 8s, 16s...
                        wait_time = min(2 ** (retry_count - 1), 16)  # Max 16 seconds
                        print(f"[DEBUG] Waiting {wait_time}s before retry...", flush=True)
                        await asyncio.sleep(wait_time)
                    else:
                        # All retries exhausted
                        print(f"[ERROR] Failed to connect to CDP after {BROWSER_CONNECT_MAX_RETRIES} attempts", flush=True)
                        raise HTTPException(
                            status_code=503,
                            detail=f"Failed to connect to browser at {CDP_URL} after {BROWSER_CONNECT_MAX_RETRIES} attempts: {str(last_error)}"
                        )

    return _browser


async def get_page():
    """Get or create persistent page instance."""
    global _page

    browser = await get_browser()

    if _page is None:
        contexts = browser.contexts
        if contexts and contexts[0].pages:
            _page = contexts[0].pages[0]
        else:
            context = contexts[0] if contexts else await browser.new_context()
            _page = await context.new_page()

    return _page


async def close_browser():
    """Close browser and cleanup."""
    global _playwright, _browser, _page

    if _browser:
        try:
            await _browser.close()
        except:
            pass
        _browser = None
        _page = None

    if _playwright:
        try:
            await _playwright.__aexit__(None, None, None)
        except:
            pass
        _playwright = None


# ============================================================================
# FastAPI Application
# ============================================================================

@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan handler."""
    # Initialize browser on startup
    print("[INFO] Initializing browser connection on startup...", flush=True)
    try:
        await get_browser()
        print("[INFO] Browser initialized successfully on startup", flush=True)
    except Exception as e:
        print(f"[WARN] Browser initialization failed on startup: {e}", flush=True)
        print("[WARN] Browser will be initialized on first tool execution", flush=True)

    yield

    # Cleanup on shutdown
    await close_browser()


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


# Cache for loaded modules
_module_cache: Dict[str, Any] = {}


def load_tool_module(script_path: str):
    """Dynamically load a tool module from script path."""
    if script_path in _module_cache:
        return _module_cache[script_path]

    if not os.path.exists(script_path):
        raise HTTPException(status_code=404, detail=f"Script not found: {script_path}")

    spec = importlib.util.spec_from_file_location("tool_module", script_path)
    if not spec or not spec.loader:
        raise HTTPException(status_code=500, detail=f"Cannot load module: {script_path}")

    module = importlib.util.module_from_spec(spec)
    sys.modules["tool_module"] = module
    spec.loader.exec_module(module)

    _module_cache[script_path] = module
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

        # Check if this is a browser_tools.py function that needs browser/page
        if hasattr(module, 'BROWSER_TOOLS') and action in module.BROWSER_TOOLS:
            # This is a browser automation function
            browser = await get_browser()
            page = await get_page()
            result = await module.execute_browser_tool(action, args, browser, page)
        else:
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
            stderr=asyncio.subprocess.PIPE
        )

        stdout, stderr = await proc.communicate(json.dumps(input_data).encode())

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


@app.post("/exec")
async def execute(request: ExecuteRequest):
    """
    Generic execution endpoint - works with ANY tool.

    This is the only endpoint needed. It dynamically loads and executes
    the requested tool without any hardcoded tool names.

    Browser tools are executed via direct Python function calls (no subprocess).
    Legacy Node.js tools use subprocess (will be phased out).
    """
    try:
        if request.interpreter in ("python", "python3"):
            result = await execute_python_tool(
                request.script_path,
                request.action,
                request.args,
                request.environment
            )
            return result
        elif request.interpreter == "node":
            result = await execute_node_tool(
                request.script_path,
                request.action,
                request.args
            )
            return result
        else:
            result = await execute_subprocess(
                request.interpreter,
                [request.script_path, request.action],
                request.args
            )
            return result

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
        "display": DISPLAY,
        "cdp_url": CDP_URL,
        "browser_connected": _browser is not None,
        "browser_initialized": _browser_initialized
    }


# ============================================================================
# Main entry point
# ============================================================================

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8080)
