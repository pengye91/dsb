"""
DSB SDK - Python client for Distributed Sandboxes

This SDK provides both sync and async APIs for interacting with:
- DSB Server (sandbox management)
- SSH Gateway (session management)
- Web scraping and browser automation
"""

__version__ = "0.1.0"

from dsb_sdk.api.activities import ActivitiesAPI
from dsb_sdk.api.activities_async import AsyncActivitiesAPI
from dsb_sdk.api.health import HealthAPI
from dsb_sdk.api.health_async import AsyncHealthAPI
from dsb_sdk.api.sandbox import SandboxAPI
from dsb_sdk.api.sandbox_async import AsyncSandboxAPI
from dsb_sdk.api.ssh import SSHAPI
from dsb_sdk.api.ssh_async import AsyncSSHAPI
from dsb_sdk.api.static_files import StaticFilesAPI
from dsb_sdk.api.static_files_async import AsyncStaticFilesAPI
from dsb_sdk.api.terminal import AsyncTerminalAPI, TerminalAPI
from dsb_sdk.api.web import WebAPI
from dsb_sdk.api.web_async import AsyncWebAPI
from dsb_sdk.client import AsyncDSBClient, DSBClient
from dsb_sdk.config import DSBConfig
from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBConnectionError,
    DSBError,
    DSBTimeoutError,
    DSBValidationError,
)
from dsb_sdk.types.common import Activity, ActivityListResponse, HealthStatus
from dsb_sdk.types.exec import ExecRequest, ExecResponse
from dsb_sdk.types.sandbox import (
    PullPolicy,
    ResourceLimits,
    Sandbox,
    SandboxConfig,
    SandboxProgressEvent,
    SandboxState,
    SandboxStats,
    StaticFileList,
    StaticFileMetadata,
)
from dsb_sdk.types.ssh import SSHSession, SSHSessionConfig
from dsb_sdk.types.web import (
    BrowserAction,
    BrowserActionResponse,
    BrowserInfo,
    WebCrawlResponse,
    WebFormat,
    WebHealthResponse,
    WebLinksResponse,
    WebScrapeResult,
    WebScreenshotFormat,
    WebTableResult,
)

__all__ = [
    "DSBClient",
    "AsyncDSBClient",
    "DSBConfig",
    "SandboxAPI",
    "AsyncSandboxAPI",
    "SSHAPI",
    "AsyncSSHAPI",
    "HealthAPI",
    "AsyncHealthAPI",
    "ActivitiesAPI",
    "AsyncActivitiesAPI",
    "StaticFilesAPI",
    "AsyncStaticFilesAPI",
    "TerminalAPI",
    "AsyncTerminalAPI",
    "WebAPI",
    "AsyncWebAPI",
    "DSBError",
    "DSBAPIError",
    "DSBConnectionError",
    "DSBTimeoutError",
    "DSBValidationError",
    # Sandbox types
    "PullPolicy",
    "ResourceLimits",
    "SandboxProgressEvent",
    "Sandbox",
    "SandboxConfig",
    "SandboxState",
    "SandboxStats",
    "StaticFileMetadata",
    "StaticFileList",
    # SSH types
    "SSHSession",
    "SSHSessionConfig",
    # Exec types
    "ExecRequest",
    "ExecResponse",
    # Common types
    "HealthStatus",
    "Activity",
    "ActivityListResponse",
    # Web types
    "WebFormat",
    "WebScreenshotFormat",
    "WebScrapeResult",
    "WebLinksResponse",
    "WebTableResult",
    "WebCrawlResponse",
    "WebHealthResponse",
    "BrowserAction",
    "BrowserActionResponse",
    "BrowserInfo",
]
