"""
DSB Server Lifecycle Manager

Provides a context manager for starting and stopping the DSB server
during tests, with health check and startup timeout support.
"""

import logging
import os
import signal
import subprocess
import time
from pathlib import Path

import pytest

server_logger = logging.getLogger("dsb_tests.server_manager")


class DSBServerManager:
    """
    Manages the lifecycle of a DSB server for testing.

    Provides context manager protocol for automatic startup and shutdown.
    Includes health check and startup timeout support.

    Example:
        ```python
        from dsb_server_manager import DSBServerManager

        async with DSBServerManager() as server:
            # Server is running and healthy
            assert server.is_running()
            api_url = server.api_url
        # Server is stopped
        ```
    """

    def __init__(
        self,
        env_file: Path | str | None = None,
        startup_timeout: float = 60.0,
        port: int = 8080,
    ):
        """
        Initialize the DSB server manager.

        Args:
            env_file: Path to .env file to source (default: .env.test)
            startup_timeout: Maximum time to wait for server startup (seconds)
            port: Port number for the DSB server
        """
        self.env_file = Path(env_file) if env_file else Path(".env.test")
        self.startup_timeout = startup_timeout
        self.port = port
        self.process: subprocess.Popen | None = None
        self.api_url = f"http://localhost:{port}"

    def __enter__(self) -> "DSBServerManager":
        """Start the DSB server."""
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):  # type: ignore[no-untyped-def]
        """Stop the DSB server."""
        self.stop()
        return False

    def start(self) -> None:
        """
        Start the DSB server.

        Raises:
            RuntimeError: If server fails to start or becomes healthy
        """
        if self.is_running():
            server_logger.warning(f"DSB server already running at {self.api_url}")
            return

        server_logger.info(f"Starting DSB server at {self.api_url}")

        # Build the DSB server if needed
        self._build_server()

        # Source env file and start server
        env = self._load_env()

        try:
            # Start the DSB server process
            self.process = subprocess.Popen(
                ["cargo", "run", "--bin", "dsb"],
                cwd=self._get_dsb_root(),
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                preexec_fn=os.setsid if hasattr(os, "setsid") else None,
            )

            # Wait for server to become healthy
            if not self._wait_for_healthy():
                self.stop()
                raise RuntimeError(f"DSB server failed to start within {self.startup_timeout}s")

            server_logger.info(f"DSB server started successfully at {self.api_url}")

        except Exception as e:
            self.stop()
            raise RuntimeError(f"Failed to start DSB server: {e}") from e

    def stop(self) -> None:
        """Stop the DSB server."""
        if self.process is None:
            return

        server_logger.info("Stopping DSB server")

        try:
            # Try graceful shutdown first
            if hasattr(os, "killpg"):
                os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
            else:
                self.process.terminate()

            # Wait for process to exit
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                # Force kill if graceful shutdown failed
                if hasattr(os, "killpg"):
                    os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
                else:
                    self.process.kill()
                self.process.wait()

            server_logger.info("DSB server stopped")

        except Exception as e:
            server_logger.error(f"Error stopping DSB server: {e}")
        finally:
            self.process = None

    def is_running(self) -> bool:
        """
        Check if the DSB server is running.

        Returns:
            True if server process is running and healthy
        """
        if self.process is None:
            return False

        # Check if process is still alive
        if self.process.poll() is not None:
            return False

        # Check health endpoint
        try:
            import urllib.request

            with urllib.request.urlopen(f"{self.api_url}/health", timeout=2) as response:
                data = response.read().decode()
                return "healthy" in data.lower() or "ok" in data.lower()
        except Exception:
            return False

    def _build_server(self) -> None:
        """Build the DSB server binary if needed."""
        dsb_root = self._get_dsb_root()

        # Check if binary exists
        debug_binary = dsb_root / "target" / "debug" / "dsb"

        if not debug_binary.exists():
            server_logger.info("Building DSB server...")
            try:
                subprocess.run(
                    ["cargo", "build", "--bin", "dsb"],
                    cwd=dsb_root,
                    check=True,
                    capture_output=True,
                )
                server_logger.info("DSB server built successfully")
            except subprocess.CalledProcessError as e:
                raise RuntimeError(f"Failed to build DSB server: {e}") from e

    def _load_env(self) -> dict[str, str]:
        """
        Load environment variables from .env file.

        Returns:
            Dictionary of environment variables
        """
        env = os.environ.copy()

        if not self.env_file.exists():
            server_logger.warning(f"Env file not found: {self.env_file}")
            return env

        server_logger.info(f"Loading env from: {self.env_file}")

        try:
            with open(self.env_file) as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith("#") and "=" in line:
                        key, value = line.split("=", 1)
                        env[key.strip()] = value.strip()
        except Exception as e:
            server_logger.error(f"Error loading env file: {e}")

        return env

    def _wait_for_healthy(self) -> bool:
        """
        Wait for the server to become healthy.

        Returns:
            True if server became healthy within timeout
        """
        import urllib.request

        start = time.time()
        while time.time() - start < self.startup_timeout:
            try:
                with urllib.request.urlopen(f"{self.api_url}/health", timeout=1) as response:
                    data = response.read().decode()
                    if "healthy" in data.lower() or "ok" in data.lower():
                        return True
            except Exception:
                pass

            time.sleep(0.5)

        return False

    def _get_dsb_root(self) -> Path:
        """
        Get the DSB project root directory.

        Returns:
            Path to DSB root directory
        """
        # Start from current file and go up to find DSB root
        current = Path(__file__).parent
        while current != current.parent:
            if (current / "Cargo.toml").exists():
                return current
            current = current.parent

        # Default to relative path from tests directory
        return Path(__file__).parent.parent.parent


# ============================================================================
# Session-scoped fixture for pytest
# ============================================================================


@pytest.fixture(scope="session")
def dsb_server():  # type: ignore[no-untyped-def]
    """
    Session-scoped fixture that manages the DSB server lifecycle.

    This fixture starts the server at the beginning of the test session
    and stops it after all tests complete.

    Example:
        ```python
        def test_with_server(dsb_server: DSBServerManager):
            client = DSBClient(api_url=dsb_server.api_url)
            # Test code here
        ```
    """
    server = DSBServerManager()
    server.start()
    yield server
    server.stop()
