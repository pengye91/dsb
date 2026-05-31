"""
Command execution type definitions
"""

from __future__ import annotations

from pydantic import BaseModel, Field


class ExecRequest(BaseModel):
    """Request to execute a command"""

    command: list[str] = Field(..., description="Command and arguments to execute")
    working_dir: str | None = Field(None, description="Working directory")
    environment: dict[str, str] = Field(default_factory=dict, description="Environment variables")
    timeout: int | None = Field(None, description="Timeout in seconds")


class ExecResponse(BaseModel):
    """Response from command execution"""

    output: str = Field(..., description="Command stdout output")
    exit_code: int = Field(..., description="Exit code")

    def is_successful(self) -> bool:
        """Check if command executed successfully."""
        return self.exit_code == 0 and not self.timed_out

    def get_error_details(self) -> str:
        """Get formatted error details from stderr/stdout."""
        parts = []
        if self.stderr:
            parts.append(f"Stderr:\n{self.stderr[:500]}")
        if self.output and self.output != "None":
            parts.append(f"Stdout:\n{self.output[:500]}")
        if self.timed_out:
            parts.append("Status: Timed out")
        return "\n".join(parts) if parts else "No error details available"
