"""
Configuration for integration tests

Integration tests require a running DSB server.
"""

import os
import socket
import threading
import time
from functools import partial
from http.server import HTTPServer, SimpleHTTPRequestHandler
from pathlib import Path

import pytest

# DSB server configuration
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")

# Docker image for testing
TEST_IMAGE = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")
FIXTURES_DIR = Path("/workspace/sdks/python/tests/fixtures")


def _find_free_port() -> int:
    """Find an available TCP port in the test runner container."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("0.0.0.0", 0))
        return sock.getsockname()[1]


@pytest.fixture(scope="module")
def local_fixture_server_url() -> str:
    """Serve local HTML fixtures over HTTP for sandbox-accessible scraping tests."""
    if not os.getenv("DOCKER_COMPOSE_TEST"):
        pytest.skip("Local fixture server only available in Docker Compose test environment")

    if not FIXTURES_DIR.exists():
        pytest.skip("Fixtures directory not found")

    port = _find_free_port()
    handler = partial(SimpleHTTPRequestHandler, directory=str(FIXTURES_DIR))
    server = HTTPServer(("0.0.0.0", port), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    time.sleep(0.2)

    try:
        yield f"http://dsb-python-test-runner:{port}/test-tables.html"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def pytest_configure(config):
    """Configure pytest for integration tests"""
    config.addinivalue_line(
        "markers",
        "integration: mark test as integration test (requires running DSB server)",
    )
    config.addinivalue_line(
        "markers",
        "requires_databend: mark test as requiring Databend instance",
    )
    config.addinivalue_line(
        "markers",
        "images: mark test as Images API test",
    )
