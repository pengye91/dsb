"""
Pydantic Model Fixtures for DSB SDK Tests

Provides factory functions for creating valid Pydantic model instances.
These fixtures are essential for testing complex nested models that would
otherwise require verbose inline data construction.

Usage:
    from tests.fixtures.model_fixtures import (
        create_sandbox,
        create_sandbox_config,
        create_resource_limits,
        create_databend_config,
    )

    # Create a minimal valid sandbox
    sandbox = create_sandbox()

    # Create with custom values
    sandbox = create_sandbox(state=SandboxState.ERROR)

    # Create with nested config
    sandbox = create_sandbox(
        config=create_sandbox_config(
            image="python:3.12",
            resource_limits=create_resource_limits(memory_mb=512)
        )
    )
"""

from datetime import UTC, datetime
from typing import Any
from uuid import UUID, uuid4

from dsb_sdk.types.exec import ExecRequest, ExecResponse
from dsb_sdk.types.sandbox import (
    DatabendConfig,
    FileDownloadResponse,
    FileInfo,
    PaginationMeta,
    PullPolicy,
    ResourceLimits,
    Sandbox,
    SandboxConfig,
    SandboxCreateRequest,
    SandboxListResponse,
    SandboxProgressEvent,
    SandboxState,
    SandboxStats,
    StaticFileList,
    StaticFileMetadata,
    Ulimit,
    UploadFileResponse,
)
from dsb_sdk.types.ssh import SSHSession, SSHSessionConfig
from dsb_sdk.types.web import (
    BrowserActionResponse,
    BrowserInfo,
    BrowserTabInfo,
    WebCrawlResponse,
    WebCrawlResult,
    WebHealthResponse,
    WebScrapeResult,
)

# ============================================================================
# Sandbox Model Fixtures
# ============================================================================


def create_ulimit(
    name: str = "nofile",
    soft: int = 65536,
    hard: int = 65536,
) -> Ulimit:
    """Create a valid Ulimit instance."""
    return Ulimit(name=name, soft=soft, hard=hard)


def create_resource_limits(
    memory_mb: float | None = 512.0,
    cpu_quota: int | None = None,
    cpu_period: int | None = None,
    cpu_shares: int | None = None,
    pids_limit: int | None = None,
    ulimits: list[Ulimit] | None = None,
) -> ResourceLimits:
    """Create a valid ResourceLimits instance."""
    return ResourceLimits(
        memory_mb=memory_mb,
        cpu_quota=cpu_quota,
        cpu_period=cpu_period,
        cpu_shares=cpu_shares,
        pids_limit=pids_limit,
        ulimits=ulimits,
    )


def create_databend_config(
    host: str = "databend.example.com",
    port: int = 8000,
    user: str | None = "admin",
    password: str | None = "secret",
    database: str = "default",
    virtual_db_prefix: str = "compliance_virtual_cluster",
    meta_path: str = "/opt/tools/meta/compliance_table_meta.xml",
) -> DatabendConfig:
    """Create a valid DatabendConfig instance."""
    return DatabendConfig(
        host=host,
        port=port,
        user=user,
        password=password,
        database=database,
        virtual_db_prefix=virtual_db_prefix,
        meta_path=meta_path,
    )


def create_sandbox_config(
    image: str = "python:3.12",
    name: str | None = None,
    environment: dict[str, str] | None = None,
    port_mappings: dict[str, str] | None = None,
    volumes: dict[str, str] | None = None,
    command: list[str] | None = None,
    pull_policy: PullPolicy | None = None,
    resource_limits: ResourceLimits | None = None,
    inactivity_timeout_minutes: int | None = None,
    features: list[str] | None = None,
    enable_all_features: bool = False,
    databend: DatabendConfig | None = None,
) -> SandboxConfig:
    """Create a valid SandboxConfig instance."""
    return SandboxConfig(
        image=image,
        name=name,
        environment=environment or {},
        port_mappings=port_mappings or {},
        volumes=volumes or {},
        command=command,
        pull_policy=pull_policy,
        resource_limits=resource_limits,
        inactivity_timeout_minutes=inactivity_timeout_minutes,
        features=features or [],
        enable_all_features=enable_all_features,
        databend=databend,
    )


def create_sandbox(
    id: UUID | None = None,
    config: SandboxConfig | None = None,
    state: SandboxState = SandboxState.RUNNING,
    container_id: str | None = None,
    created_at: datetime | None = None,
    updated_at: datetime | None = None,
    deleted_at: datetime | None = None,
    deleted_by: str | None = None,
) -> Sandbox:
    """Create a valid Sandbox instance."""
    now = datetime.now(UTC)
    return Sandbox(
        id=id or uuid4(),
        config=config or create_sandbox_config(),
        state=state,
        container_id=container_id or "abc123def456",
        created_at=created_at or now,
        updated_at=updated_at or now,
        deleted_at=deleted_at,
        deleted_by=deleted_by,
    )


def create_sandbox_stats(
    sandbox_id: UUID | None = None,
    cpu_percent: float = 15.5,
    memory_usage_mb: float = 256.0,
    memory_limit_mb: float = 512.0,
    memory_percent: float = 50.0,
    network_rx_bytes: int = 1024,
    network_tx_bytes: int = 2048,
    block_read_bytes: int = 4096,
    block_write_bytes: int = 8192,
    timestamp: datetime | None = None,
) -> SandboxStats:
    """Create a valid SandboxStats instance."""
    return SandboxStats(
        sandbox_id=sandbox_id,
        cpu_percent=cpu_percent,
        memory_usage_mb=memory_usage_mb,
        memory_limit_mb=memory_limit_mb,
        memory_percent=memory_percent,
        network_rx_bytes=network_rx_bytes,
        network_tx_bytes=network_tx_bytes,
        block_read_bytes=block_read_bytes,
        block_write_bytes=block_write_bytes,
        timestamp=timestamp or datetime.now(UTC),
    )


def create_sandbox_create_request(
    image: str = "python:3.12",
    name: str | None = None,
    environment: dict[str, str] | None = None,
    port_mappings: list[dict[str, Any]] | None = None,
    volumes: list[dict[str, Any]] | None = None,
    command: list[str] | None = None,
    pull_policy: str | None = None,
    resource_limits: ResourceLimits | None = None,
    inactivity_timeout_minutes: int | None = None,
    features: list[str] | None = None,
    enable_all_features: bool = False,
) -> SandboxCreateRequest:
    """Create a valid SandboxCreateRequest instance."""
    return SandboxCreateRequest(
        image=image,
        name=name,
        environment=environment,
        port_mappings=port_mappings,
        volumes=volumes,
        command=command,
        pull_policy=pull_policy,
        resource_limits=resource_limits,
        inactivity_timeout_minutes=inactivity_timeout_minutes,
        features=features,
        enable_all_features=enable_all_features,
    )


def create_sandbox_progress_event(
    event: str = "ready",
    message: str = "Sandbox is ready",
    progress: int = 100,
    details: str | None = None,
) -> SandboxProgressEvent:
    """Create a valid SandboxProgressEvent instance."""
    return SandboxProgressEvent(
        event=event,
        message=message,
        progress=progress,
        details=details,
    )


def create_pagination_meta(
    page: int = 1,
    per_page: int = 50,
    total: int = 0,
    total_pages: int = 0,
    has_next: bool = False,
    has_prev: bool = False,
) -> PaginationMeta:
    """Create a valid PaginationMeta instance."""
    return PaginationMeta(
        page=page,
        per_page=per_page,
        total=total,
        total_pages=total_pages,
        has_next=has_next,
        has_prev=has_prev,
    )


def create_sandbox_list_response(
    data: list[Sandbox] | None = None,
    pagination: PaginationMeta | None = None,
) -> SandboxListResponse:
    """Create a valid SandboxListResponse instance."""
    return SandboxListResponse(
        data=data or [],
        pagination=pagination or create_pagination_meta(),
    )


def create_static_file_metadata(
    file_name: str = "example.txt",
    file_path: str = "/static/example.txt",
    file_size_bytes: int = 1024,
    content_type: str = "text/plain",
) -> StaticFileMetadata:
    """Create a valid StaticFileMetadata instance."""
    return StaticFileMetadata(
        file_name=file_name,
        file_path=file_path,
        file_size_bytes=file_size_bytes,
        content_type=content_type,
    )


def create_static_file_list(
    sandbox_id: UUID | None = None,
    files: list[StaticFileMetadata] | None = None,
    total_count: int = 0,
    total_size_bytes: int = 0,
) -> StaticFileList:
    """Create a valid StaticFileList instance."""
    return StaticFileList(
        sandbox_id=sandbox_id or uuid4(),
        files=files or [],
        total_count=total_count,
        total_size_bytes=total_size_bytes,
    )


def create_file_info(
    name: str = "test.txt",
    path: str = "/app/test.txt",
    size: int = 1024,
    uploaded_at: datetime | None = None,
) -> FileInfo:
    """Create a valid FileInfo instance."""
    return FileInfo(
        name=name,
        path=path,
        size=size,
        uploaded_at=uploaded_at or datetime.now(UTC),
    )


def create_upload_file_response(
    success: bool = True,
    file: FileInfo | None = None,
) -> UploadFileResponse:
    """Create a valid UploadFileResponse instance."""
    return UploadFileResponse(
        success=success,
        file=file or create_file_info(),
    )


def create_file_download_response(
    name: str = "test.txt",
    path: str = "/app/test.txt",
    size: int = 1024,
    content_type: str = "text/plain",
    content: bytes = b"test content",
) -> FileDownloadResponse:
    """Create a valid FileDownloadResponse instance."""
    return FileDownloadResponse(
        name=name,
        path=path,
        size=size,
        content_type=content_type,
        content=content,
    )


# ============================================================================
# SSH Model Fixtures
# ============================================================================


def create_ssh_session_config(
    sandbox_id: UUID | None = None,
    client_ip: str = "127.0.0.1",
    auth_method: str = "api_key",
    username: str | None = None,
    public_key: str | None = None,
    ssh_version: str | None = None,
) -> SSHSessionConfig:
    """Create a valid SSHSessionConfig instance."""
    return SSHSessionConfig(
        sandbox_id=sandbox_id or uuid4(),
        client_ip=client_ip,
        auth_method=auth_method,
        username=username,
        public_key=public_key,
        ssh_version=ssh_version,
    )


def create_ssh_session(
    id: UUID | None = None,
    sandbox_id: UUID | None = None,
    state: str = "active",
    username: str | None = "testuser",
    connected_at: datetime | None = None,
    last_activity_at: datetime | None = None,
) -> SSHSession:
    """Create a valid SSHSession instance.

    Uses aliases connected_at and last_activity_at which map to
    created_at and last_activity respectively.
    """
    now = datetime.now(UTC)
    return SSHSession(
        id=id or uuid4(),
        sandbox_id=sandbox_id or uuid4(),
        state=state,
        username=username,
        connected_at=connected_at or now,
        last_activity_at=last_activity_at or now,
    )


# ============================================================================
# Exec Model Fixtures
# ============================================================================


def create_exec_response(
    exit_code: int = 0,
    output: str = "",
    stderr: str = "",
    timed_out: bool = False,
) -> ExecResponse:
    """Create a valid ExecResponse instance."""
    return ExecResponse(
        exit_code=exit_code,
        output=output,
        stderr=stderr,
        timed_out=timed_out,
    )


def create_exec_request(
    command: list[str],
    working_dir: str | None = None,
    environment: dict[str, str] | None = None,
    timeout: int | None = None,
) -> ExecRequest:
    """Create a valid ExecRequest instance."""
    return ExecRequest(
        command=command,
        working_dir=working_dir,
        environment=environment or {},
        timeout=timeout,
    )


# ============================================================================
# Web Model Fixtures
# ============================================================================


def create_browser_info(
    supports_automation: bool = True,
    browser_type: str | None = "chromium",
    cdp_port: int | None = None,
    image_name: str | None = None,
) -> BrowserInfo:
    """Create a valid BrowserInfo instance."""
    return BrowserInfo(
        supports_automation=supports_automation,
        browser_type=browser_type,
        cdp_port=cdp_port,
        image_name=image_name,
    )


def create_web_scrape_result(
    url: str = "https://example.com",
    title: str = "Example Domain",
    content: str = "Example content",
    screenshot: str | None = None,
    screenshot_encoding: str | None = None,
    screenshot_path: str | None = None,
) -> WebScrapeResult:
    """Create a valid WebScrapeResult instance."""
    return WebScrapeResult(
        url=url,
        title=title,
        content=content,
        screenshot=screenshot,
        screenshot_encoding=screenshot_encoding,
        screenshot_path=screenshot_path,
    )


def create_web_crawl_result(
    url: str = "https://example.com",
    success: bool = True,
    title: str = "Example",
    content: str = "Example content",
    link_count: int = 5,
    error: str | None = None,
) -> WebCrawlResult:
    """Create a valid WebCrawlResult instance."""
    return WebCrawlResult(
        url=url,
        success=success,
        title=title,
        content=content,
        link_count=link_count,
        error=error,
    )


def create_web_crawl_response(
    total_urls: int = 10,
    successful: int = 8,
    failed: int = 2,
    results: list[WebCrawlResult] | None = None,
) -> WebCrawlResponse:
    """Create a valid WebCrawlResponse instance."""
    return WebCrawlResponse(
        total_urls=total_urls,
        successful=successful,
        failed=failed,
        results=results or [],
    )


def create_web_health_response(
    message: str = "Browser is ready",
    cdp_url: str = "ws://localhost:9222",
    browser_ready: bool = True,
) -> WebHealthResponse:
    """Create a valid WebHealthResponse instance."""
    return WebHealthResponse(
        message=message,
        cdp_url=cdp_url,
        browser_ready=browser_ready,
    )


def create_browser_tab_info(
    index: int = 0,
    title: str = "Example",
    url: str = "https://example.com",
    active: bool = True,
) -> BrowserTabInfo:
    """Create a valid BrowserTabInfo instance."""
    return BrowserTabInfo(
        index=index,
        title=title,
        url=url,
        active=active,
    )


def create_browser_action_response(
    status: str = "success",
    url: str | None = None,
    tabs: list[BrowserTabInfo] | None = None,
    elements: list[dict[str, Any]] | None = None,
    result: Any = None,
    path: str | None = None,
    error_message: str | None = None,
) -> BrowserActionResponse:
    """Create a valid BrowserActionResponse instance."""
    return BrowserActionResponse(
        status=status,
        url=url,
        tabs=tabs,
        elements=elements,
        result=result,
        path=path,
        error_message=error_message,
    )
