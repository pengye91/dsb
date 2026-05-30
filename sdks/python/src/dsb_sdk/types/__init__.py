"""Type definitions for DSB SDK"""

from dsb_sdk.types.common import HealthStatus
from dsb_sdk.types.exec import ExecRequest, ExecResponse
from dsb_sdk.types.sandbox import (
    DatabendConfig,
    FileDownloadResponse,
    FileInfo,
    PullPolicy,
    ResourceLimits,
    Sandbox,
    SandboxConfig,
    SandboxProgressEvent,
    SandboxState,
    SandboxStats,
    UploadFileResponse,
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
    "SandboxState",
    "SandboxConfig",
    "Sandbox",
    "SandboxStats",
    "PullPolicy",
    "ResourceLimits",
    "DatabendConfig",
    "SandboxProgressEvent",
    "FileInfo",
    "UploadFileResponse",
    "FileDownloadResponse",
    "SSHSession",
    "SSHSessionConfig",
    "ExecRequest",
    "ExecResponse",
    "HealthStatus",
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
