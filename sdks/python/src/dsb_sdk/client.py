"""
Main client classes for DSB SDK

Provides both synchronous and asynchronous client interfaces.
"""


from dsb_sdk.api.activities import ActivitiesAPI
from dsb_sdk.api.activities_async import AsyncActivitiesAPI
from dsb_sdk.api.admin import AdminAPI
from dsb_sdk.api.admin_async import AsyncAdminAPI
from dsb_sdk.api.config import ConfigAPI
from dsb_sdk.api.config_async import AsyncConfigAPI
from dsb_sdk.api.health import HealthAPI
from dsb_sdk.api.health_async import AsyncHealthAPI
from dsb_sdk.api.images import ImagesAPI
from dsb_sdk.api.images_async import AsyncImagesAPI
from dsb_sdk.api.sandbox import SandboxAPI
from dsb_sdk.api.sandbox_async import AsyncSandboxAPI
from dsb_sdk.api.ssh import SSHAPI
from dsb_sdk.api.ssh_async import AsyncSSHAPI
from dsb_sdk.api.static_files import StaticFilesAPI
from dsb_sdk.api.static_files_async import AsyncStaticFilesAPI
from dsb_sdk.api.terminal import AsyncTerminalAPI, TerminalAPI
from dsb_sdk.api.web import WebAPI
from dsb_sdk.api.web_async import AsyncWebAPI
from dsb_sdk.logging import get_logger
from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.transport.sync import SyncTransport

logger = get_logger("dsb_sdk.client")


class DSBClient:
    """
    Synchronous client for DSB API.

    Example:
        >>> from dsb_sdk import DSBClient
        >>> client = DSBClient(api_url="http://localhost:8080")
        >>> sandbox = client.sandbox.create(image="python:3.12", name="test")
        >>> print(sandbox.id)
    """

    def __init__(
        self,
        api_url: str = "http://localhost:8080",
        timeout: float = 30.0,
        verify_ssl: bool = True,
        api_key: str | None = None,
    ):
        """
        Initialize synchronous DSB client.

        Args:
            api_url: Base URL for DSB API (default: "http://localhost:8080")
            timeout: Request timeout in seconds (default: 30.0)
            verify_ssl: Whether to verify SSL certificates (default: True)
            api_key: Optional API key for authentication (default: None)
        """
        # Log client initialization
        logger.info(
            "sync_client_initialized",
            api_url=api_url,
            timeout=timeout,
            verify_ssl=verify_ssl,
            has_api_key=api_key is not None,
        )

        self._transport = SyncTransport(
            api_url=api_url,
            timeout=timeout,
            verify_ssl=verify_ssl,
            api_key=api_key,
        )

        # Initialize API modules
        self.sandbox = SandboxAPI(self._transport)
        self.ssh = SSHAPI(self._transport)
        self.health = HealthAPI(self._transport)
        self.activities = ActivitiesAPI(self._transport)
        self.terminal = TerminalAPI(self._transport, api_url)
        self.web = WebAPI(self._transport)
        self.static_files = StaticFilesAPI(self._transport)
        self.config = ConfigAPI(self._transport)
        self.images = ImagesAPI(self._transport)
        self.admin = AdminAPI(self._transport)

    def close(self) -> None:
        """Close the client and cleanup resources."""
        self._transport.close()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()
        return False


class AsyncDSBClient:
    """
    Asynchronous client for DSB API.

    Provides identical API to DSBClient but with async methods.

    Example:
        >>> from dsb_sdk import AsyncDSBClient
        >>> async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        ...     sandbox = await client.sandbox.create_async(image="python:3.12")
        ...     print(sandbox.id)
    """

    def __init__(
        self,
        api_url: str = "http://localhost:8080",
        timeout: float = 30.0,
        verify_ssl: bool = True,
        api_key: str | None = None,
    ):
        """
        Initialize asynchronous DSB client.

        Args:
            api_url: Base URL for DSB API (default: "http://localhost:8080")
            timeout: Request timeout in seconds (default: 30.0)
            verify_ssl: Whether to verify SSL certificates (default: True)
            api_key: Optional API key for authentication (default: None)
        """
        # Log async client initialization
        logger.info(
            "async_client_initialized",
            api_url=api_url,
            timeout=timeout,
            verify_ssl=verify_ssl,
            has_api_key=api_key is not None,
        )

        self._transport = AsyncTransport(
            api_url=api_url,
            timeout=timeout,
            verify_ssl=verify_ssl,
            api_key=api_key,
        )

        # Initialize async API modules
        self.sandbox = AsyncSandboxAPI(self._transport)
        self.ssh = AsyncSSHAPI(self._transport)
        self.health = AsyncHealthAPI(self._transport)
        self.activities = AsyncActivitiesAPI(self._transport)
        self.terminal = AsyncTerminalAPI(self._transport, api_url)
        self.web = AsyncWebAPI(self._transport)
        self.static_files = AsyncStaticFilesAPI(self._transport)
        self.config = AsyncConfigAPI(self._transport)
        self.images = AsyncImagesAPI(self._transport)
        self.admin = AsyncAdminAPI(self._transport)

    async def close(self) -> None:
        """Close the client and cleanup resources."""
        await self._transport.close()

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()
        return False
