"""
Sandbox API implementation (asynchronous)

Provides asynchronous methods for managing sandboxes.
Use with AsyncDSBClient.
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
from typing import Any, BinaryIO
from uuid import UUID

from dsb_sdk.exceptions import DSBAPIError
from dsb_sdk.logging import LoggingContext, get_logger
from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.types.sandbox import (
    DatabendConfig,
    FileDownloadResponse,
    PaginationMeta,
    PullPolicy,
    ResourceLimits,
    Sandbox,
    SandboxCreateRequest,
    SandboxListResponse,
    SandboxState,
    SandboxStats,
    UploadFileResponse,
)

logger = get_logger(__name__)


class AsyncSandboxAPI:
    """
    API for managing sandboxes (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.

    Example:
        >>> async with AsyncDSBClient() as client:
        ...     sandbox = await client.sandbox.create_async(image="python:3.12")
        ...     result = await client.sandbox.exec_async(sandbox.id, ["ls", "-la"])
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async sandbox API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def create_async(
        self,
        image: str,
        name: str | None = None,
        environment: dict[str, str] | None = None,
        ports: dict[str, str] | None = None,
        volumes: dict[str, str] | None = None,
        command: list[str] | None = None,
        pull_policy: PullPolicy | None = None,
        resource_limits: ResourceLimits | None = None,
        inactivity_timeout_minutes: int | None = None,
        features: list[str] | None = None,
        enable_all_features: bool = False,
        databend: DatabendConfig | None = None,
    ) -> Sandbox:
        """
        Create a new sandbox with lifecycle logging.

        Args:
            image: Docker image (e.g., "python:3.12")
            name: Optional sandbox name
            environment: Environment variables
            ports: Port mappings (e.g., {"8080": "80"})
            volumes: Volume mounts (e.g., {"/host/path": "/container/path"})
            command: Override default command
            pull_policy: Image pull policy (always, missing, never)
            resource_limits: Resource limits (memory_mb, cpu_quota, etc.)
            inactivity_timeout_minutes: Auto-stop after inactivity (minutes)
            features: Feature profiles to enable from image metadata
            enable_all_features: Enable all default features from image metadata
            databend: Databend configuration (auto-injects credentials as env vars)

        Returns:
            Created Sandbox instance

        Raises:
            DSBValidationError: Invalid parameters
            DSBAPIError: API error
            DSBConnectionError: Connection error
        """
        # Use LoggingContext for operation tracking
        with LoggingContext(operation="create_sandbox"):
            logger.info(
                "sandbox_create_start",
                image=image,
                name=name,
            )

            try:
                # Format port mappings as API expects: [{"host_port": 8080, "container_port": 80, "protocol": "tcp"}]
                port_mappings = [
                    {"host_port": int(host_port), "container_port": int(container_port), "protocol": "tcp"}
                    for host_port, container_port in (ports or {}).items()
                ] if ports else []

                # Format volumes as API expects: [{"type": "bind", "host_path": "/host", "container_path": "/container", "read_only": false}]
                volume_mounts = [
                    {"type": "bind", "host_path": source, "container_path": target, "read_only": False}
                    for source, target in (volumes or {}).items()
                ] if volumes else []

                # Convert resource_limits to dict with integer values
                resource_limits_dict = None
                if resource_limits:
                    limits_dict = resource_limits.model_dump(exclude_none=True)
                    # Convert float values to int
                    if "memory_mb" in limits_dict and limits_dict["memory_mb"] is not None:
                        limits_dict["memory_mb"] = int(limits_dict["memory_mb"])
                    # Always include ulimits as empty list if not present
                    if "ulimits" not in limits_dict:
                        limits_dict["ulimits"] = []
                    elif limits_dict.get("ulimits") is None:
                        limits_dict["ulimits"] = []
                    resource_limits_dict = limits_dict

                # Merge Databend credentials into environment
                final_environment = environment or {}
                if databend:
                    databend_env = databend.to_environment_dict()
                    final_environment = {**final_environment, **databend_env}

                request_data = {
                    "image": image,
                    "name": name,
                    "environment": final_environment,
                    "port_mappings": port_mappings,
                    "volumes": volume_mounts,
                    "command": command,
                    "pull_policy": pull_policy.value if pull_policy else None,
                    "resource_limits": resource_limits_dict,
                    "inactivity_timeout_minutes": inactivity_timeout_minutes,
                    "features": features or [],
                    "enable_all_features": enable_all_features,
                }

                response = await self.transport.request(
                    method="POST",
                    path="/sandboxes",
                    json_data={
                        k: v for k, v in request_data.items() if v is not None and v != [] and v != {}
                    },
                )

                sandbox = Sandbox(**response)

                logger.info(
                    "sandbox_created",
                    sandbox_id=sandbox.id,
                    image=sandbox.config.image,
                    state=sandbox.state.value if sandbox.state else None,
                )

                return sandbox

            except Exception as e:
                logger.error(
                    "sandbox_create_failed",
                    error=str(e),
                    error_type=type(e).__name__,
                    image=image,
                    name=name,
                )
                raise

    async def create_stream_async(
        self,
        image: str,
        name: str | None = None,
        environment: dict[str, str] | None = None,
        ports: dict[str, str] | None = None,
        volumes: dict[str, str] | None = None,
        command: list[str] | None = None,
        pull_policy: PullPolicy | None = None,
        resource_limits: ResourceLimits | None = None,
        inactivity_timeout_minutes: int | None = None,
        features: list[str] | None = None,
        enable_all_features: bool = False,
        databend: DatabendConfig | None = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """
        Create a sandbox with streaming progress updates.

        Args:
            image: Docker image
            name: Optional sandbox name
            environment: Environment variables
            ports: Port mappings
            volumes: Volume mounts
            command: Override default command
            pull_policy: Image pull policy (always, missing, never)
            resource_limits: Resource limits (memory_mb, cpu_quota, etc.)
            inactivity_timeout_minutes: Auto-stop after inactivity (minutes)
            features: Feature profiles to enable from image metadata
            enable_all_features: Enable all default features from image metadata

        Yields:
            Progress events as dictionaries

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     async for event in client.sandbox.create_stream_async("python:3.12"):
            ...         print(f"Progress: {event['stage']} - {event['message']}")
        """
        # Format port mappings as API expects
        port_mappings_list = [
            {"host_port": int(host_port), "container_port": int(container_port), "protocol": "tcp"}
            for host_port, container_port in (ports or {}).items()
        ] if ports else None

        # Format volumes as API expects
        volumes_list = [
            {"type": "bind", "host_path": source, "container_path": target, "read_only": False}
            for source, target in (volumes or {}).items()
        ] if volumes else None

        # Convert resource_limits to dict with integer values
        resource_limits_dict = None
        if resource_limits:
            limits_dict = resource_limits.model_dump(exclude_none=True)
            if "memory_mb" in limits_dict and limits_dict["memory_mb"] is not None:
                limits_dict["memory_mb"] = int(limits_dict["memory_mb"])
            # Always include ulimits as empty list if not present
            if "ulimits" not in limits_dict:
                limits_dict["ulimits"] = []
            elif limits_dict.get("ulimits") is None:
                limits_dict["ulimits"] = []
            resource_limits_dict = limits_dict

        request_data = SandboxCreateRequest(
            image=image,
            name=name,
            environment=environment or {},
            port_mappings=port_mappings_list,
            volumes=volumes_list,
            command=command,
            pull_policy=pull_policy.value if pull_policy else None,
            resource_limits=resource_limits_dict,
            inactivity_timeout_minutes=inactivity_timeout_minutes,
            features=features or [],
            enable_all_features=enable_all_features,
        )

        async for event in self.transport.stream(
            method="POST",
            path="/sandboxes/create-stream",
            json_data=request_data.model_dump(exclude_none=True),
        ):
            yield event

    async def get_async(self, sandbox_id: str, include_deleted: bool = False) -> Sandbox:
        """
        Get sandbox details with logging.

        Args:
            sandbox_id: Sandbox UUID
            include_deleted: If True, include deleted sandboxes

        Returns:
            Sandbox instance

        Raises:
            DSBAPIError: Sandbox not found
        """
        logger.debug(
            "sandbox_get_start",
            sandbox_id=sandbox_id,
            include_deleted=include_deleted,
        )

        try:
            params = {"include_deleted": "true"} if include_deleted else None
            response = await self.transport.request(
                method="GET",
                path=f"/sandboxes/{sandbox_id}",
                params=params,
            )

            sandbox = Sandbox(**response)

            logger.info(
                "sandbox_retrieved",
                sandbox_id=sandbox_id,
                state=sandbox.state.value if sandbox.state else None,
            )

            return sandbox

        except DSBAPIError as e:
            if e.status_code == 404:
                logger.warning(
                    "sandbox_not_found",
                    sandbox_id=sandbox_id,
                )
            raise

    async def list_async(
        self,
        include_deleted: bool = False,
        state: str | None = None,
        image: str | None = None,
        created_after: str | None = None,
        created_before: str | None = None,
        page: int | None = None,
        per_page: int | None = None,
    ) -> SandboxListResponse:
        """
        List sandboxes with filtering and pagination.

        Args:
            include_deleted: If True, include deleted sandboxes (default: False)
            state: Filter by state (e.g., "running", "stopped", "creating", "error")
            image: Filter by Docker image (e.g., "python:3.12")
            created_after: Filter sandboxes created after this ISO 8601 timestamp
            created_before: Filter sandboxes created before this ISO 8601 timestamp
            page: Page number (default: 1)
            per_page: Items per page (default: 50, max: 200)

        Returns:
            SandboxListResponse with list of sandboxes and pagination metadata

        Example:
            >>> # List all running sandboxes
            >>> async with AsyncDSBClient() as client:
            ...     response = await client.sandbox.list_async(state="running")
            ...     for sandbox in response.data:
            ...         print(f"{sandbox.name}: {sandbox.state}")
            >>>
            >>> # List with pagination
            ...     response = await client.sandbox.list_async(page=1, per_page=10)
            ...     print(f"Page {response.pagination.page} of {response.pagination.total_pages}")
        """
        # Build query parameters
        params: dict[str, str] = {}
        if include_deleted:
            params["include_deleted"] = "true"
        if state:
            params["state"] = state
        if image:
            params["image"] = image
        if created_after:
            params["created_after"] = created_after
        if created_before:
            params["created_before"] = created_before
        if page is not None:
            params["page"] = str(page)
        if per_page is not None:
            params["per_page"] = str(per_page)

        response = await self.transport.request(
            method="GET",
            path="/sandboxes",
            params=params if params else None,
        )

        # Handle both list and object response formats
        if isinstance(response, list):
            # Legacy format: just a list of sandboxes
            sandboxes_data = [Sandbox.model_validate(s) for s in response]
            return SandboxListResponse(
                data=sandboxes_data,
                pagination=PaginationMeta(
                    page=1,
                    per_page=len(sandboxes_data),
                    total=len(sandboxes_data),
                    total_pages=1,
                    has_next=False,
                    has_prev=False,
                ),
            )
        else:
            # Check for old format with "sandboxes" and "total" at top level
            if "sandboxes" in response and "total" in response:
                sandboxes_data = [Sandbox(**s) for s in response["sandboxes"]]
                total = response.get("total", len(sandboxes_data))
                return SandboxListResponse(
                    data=sandboxes_data,
                    pagination=PaginationMeta(
                        page=1,
                        per_page=len(sandboxes_data),
                        total=total,
                        total_pages=1,
                        has_next=False,
                        has_prev=False,
                    ),
                )

            # New format with pagination metadata
            sandboxes_data = [Sandbox(**s) for s in response.get("data", response.get("items", []))]
            pagination_data = response.get("pagination", {})
            pagination = PaginationMeta(
                page=pagination_data.get("page", 1),
                per_page=pagination_data.get("per_page", len(sandboxes_data)),
                total=pagination_data.get("total", len(sandboxes_data)),
                total_pages=pagination_data.get("total_pages", 1),
                has_next=pagination_data.get("has_next", False),
                has_prev=pagination_data.get("has_prev", False),
            )
            return SandboxListResponse(data=sandboxes_data, pagination=pagination)

    async def stop_async(self, sandbox_id: str) -> Sandbox:
        """
        Stop a running sandbox.

        Args:
            sandbox_id: Sandbox UUID

        Returns:
            Updated Sandbox instance
        """
        response = await self.transport.request(
            method="POST",
            path=f"/sandboxes/{sandbox_id}/stop",
        )
        return Sandbox(**response)

    async def start_async(self, sandbox_id: str) -> Sandbox:
        """
        Start a stopped sandbox.

        Args:
            sandbox_id: Sandbox UUID

        Returns:
            Updated Sandbox instance
        """
        response = await self.transport.request(
            method="POST",
            path=f"/sandboxes/{sandbox_id}/start",
        )
        return Sandbox(**response)

    async def delete_async(self, sandbox_id: str) -> dict[str, Any]:
        """
        Delete a sandbox.

        Args:
            sandbox_id: Sandbox UUID

        Returns:
            Deletion confirmation
        """
        return await self.transport.request(
            method="DELETE",
            path=f"/sandboxes/{sandbox_id}",
        )

    async def exec_async(
        self,
        sandbox_id: str,
        command: list[str],
        working_dir: str | None = None,
        environment: dict[str, str] | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Execute a command in a sandbox with logging.

        Args:
            sandbox_id: Sandbox UUID
            command: Command and arguments (e.g., ["echo", "hello"])
            working_dir: Working directory
            environment: Environment variables
            timeout: Timeout in seconds

        Returns:
            Execution result with output and exit code

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.sandbox.exec_async(
            ...         sandbox_id, ["python", "--version"]
            ...     )
            ...     print(result["output"])
        """
        logger.debug(
            "sandbox_exec_start",
            sandbox_id=sandbox_id,
            command=command[0] if command else None,
            timeout=timeout,
        )

        try:
            request_data = {
                "command": command,
                "working_dir": working_dir,
                "environment": environment,
                "timeout": int(timeout) if timeout is not None else None,
            }

            result = await self.transport.request(
                method="POST",
                path=f"/sandboxes/{sandbox_id}/exec",
                json_data=request_data,
            )

            logger.info(
                "sandbox_exec_complete",
                sandbox_id=sandbox_id,
                exit_code=result.get("exit_code"),
            )

            return result

        except Exception as e:
            logger.error(
                "sandbox_exec_failed",
                sandbox_id=sandbox_id,
                error=str(e),
                error_type=type(e).__name__,
            )
            raise

    async def stats_async(self, sandbox_id: str) -> SandboxStats:
        """
        Get sandbox resource usage statistics.

        Args:
            sandbox_id: Sandbox UUID

        Returns:
            SandboxStats with CPU, memory, network, disk usage
        """
        response = await self.transport.request(
            method="GET",
            path=f"/sandboxes/{sandbox_id}/stats",
        )
        return SandboxStats(**response)

    async def stats_stream_async(self, sandbox_id: str) -> AsyncIterator[dict[str, Any]]:
        """
        Stream sandbox statistics in real-time.

        Args:
            sandbox_id: Sandbox UUID

        Yields:
            Statistics updates

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     async for stats in client.sandbox.stats_stream_async(sandbox_id):
            ...         print(f"CPU: {stats['cpu_percent']}%")
        """
        async for event in self.transport.stream(
            method="GET",
            path=f"/sandboxes/{sandbox_id}/stats-stream",
        ):
            yield event

    async def upload_file_async(
        self,
        sandbox_id: str,
        path: str,
        file: BinaryIO | bytes | str,
    ) -> UploadFileResponse:
        """
        Upload a file to the sandbox filesystem (asynchronous).

        Args:
            sandbox_id: Sandbox UUID
            path: Destination path in sandbox (e.g., "/app/config.json")
            file: File to upload. Can be:
                - BinaryIO: File object (e.g., open("file.txt", "rb"))
                - bytes: Raw bytes
                - str: File path to read from

        Returns:
            UploadFileResponse with file metadata

        Raises:
            DSBValidationError: Invalid parameters
            DSBAPIError: API error (404, 409, 500, etc.)
            DSBConnectionError: Connection error
            FileNotFoundError: If file path is provided but file doesn't exist

        Example:
            >>> # Upload from file path
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.sandbox.upload_file_async(
            ...         sandbox_id,
            ...         "/app/config.json",
            ...         "local-config.json"
            ...     )

            >>> # Upload from bytes
            >>> data = b"Hello from async!"
            >>> result = await client.sandbox.upload_file_async(
            ...     sandbox_id,
            ...     "/app/message.txt",
            ...     data
            ... )
        """
        import os
        from io import BytesIO

        # Convert string to file object
        if isinstance(file, str):
            if not os.path.exists(file):
                from dsb_sdk.exceptions import DSBValidationError

                raise DSBValidationError(f"File not found: {file}")
            with open(file, "rb") as f:
                file_data = f.read()
            filename = os.path.basename(file)
            file_obj = BytesIO(file_data)
            file_obj.name = filename  # type: ignore[attr-defined]
        elif isinstance(file, bytes):
            file_obj = BytesIO(file)
            file_obj.name = "uploaded_file"  # type: ignore[attr-defined]
        else:
            # Assume it's a file-like object
            file_obj = file
            filename = getattr(file, "name", "uploaded_file")

        # Prepare multipart upload
        files = {
            "file": (getattr(file_obj, "name", "uploaded_file"), file_obj, "application/octet-stream")
        }
        data = {"path": path}

        # Reset file position if it's a file object
        if hasattr(file_obj, "seek"):
            file_obj.seek(0)

        response = await self.transport.upload_multipart(
            method="POST",
            path=f"/sandboxes/{sandbox_id}/upload",
            data=data,
            files=files,
        )

        return UploadFileResponse(**response)

    async def download_file_async(
        self,
        sandbox_id: str,
        path: str,
        disposition: str | None = None,
    ) -> FileDownloadResponse:
        """
        Download a file from the sandbox filesystem (asynchronous).

        Args:
            sandbox_id: Sandbox UUID
            path: Path to file in sandbox (e.g., "/app/config.json")
            disposition: Optional content disposition ("inline" or "attachment")

        Returns:
            FileDownloadResponse with file content and metadata

        Raises:
            DSBValidationError: Invalid parameters
            DSBAPIError: API error (404, 409, 413, 500, etc.)
            DSBConnectionError: Connection error

        Example:
            >>> # Download file asynchronously
            >>> async with AsyncDSBClient() as client:
            ...     response = await client.sandbox.download_file_async(
            ...         sandbox_id,
            ...         "/app/config.json"
            ...     )
            ...     print(f"Downloaded {response.name} ({response.size} bytes)")
        """
        import os

        # Build query parameters
        params = {"path": path}
        if disposition:
            params["disposition"] = disposition

        # Make request and get raw response
        response = await self.transport.request_bytes_async(
            method="GET",
            path=f"/sandboxes/{sandbox_id}/download",
            params=params,
        )

        # Extract headers
        content_type = response.headers.get("Content-Type", "application/octet-stream")
        filename = response.headers.get("x-file-name", os.path.basename(path))
        file_path = response.headers.get("x-file-path", path)
        file_size = int(response.headers.get("x-file-size", 0))

        # Get content
        content = response.content

        return FileDownloadResponse(
            name=filename,
            path=file_path,
            size=file_size,
            content_type=content_type,
            content=content,
        )

    async def download_file_to_path_async(
        self,
        sandbox_id: str,
        sandbox_path: str,
        local_path: str,
    ) -> dict[str, Any]:
        """
        Download a file from sandbox and save it to a local path (asynchronous).

        This is a convenience method that combines download_file_async and file writing.

        Args:
            sandbox_id: Sandbox UUID
            sandbox_path: Path to file in sandbox (e.g., "/app/data.txt")
            local_path: Local path to save the file (e.g., "./downloaded_data.txt")

        Returns:
            Dictionary with download metadata

        Raises:
            DSBValidationError: Invalid parameters
            DSBAPIError: API error
            DSBConnectionError: Connection error
            IOError: Failed to write local file

        Example:
            >>> # Download to file asynchronously
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.sandbox.download_file_to_path_async(
            ...         sandbox_id,
            ...         "/app/output.txt",
            ...         "./local_output.txt"
            ...     )
            ...     print(f"Downloaded {result['size']} bytes")
        """
        import os

        # Download file
        response = await self.download_file_async(sandbox_id, sandbox_path)

        # Ensure parent directory exists
        local_dir = os.path.dirname(local_path)
        if local_dir:
            os.makedirs(local_dir, exist_ok=True)

        # Write to file
        with open(local_path, "wb") as f:
            f.write(response.content)

        return {
            "sandbox_path": sandbox_path,
            "local_path": local_path,
            "size": response.size,
            "content_type": response.content_type,
        }

    async def wait_until_running_async(
        self,
        sandbox_id: str | UUID,
        timeout: float = 300.0,
        poll_interval: float = 1.0,
    ) -> Sandbox:
        """
        Wait for a sandbox to reach the RUNNING state.

        Args:
            sandbox_id: Sandbox UUID or string
            timeout: Maximum time to wait in seconds (default: 300)
            poll_interval: Polling interval in seconds (default: 1)

        Returns:
            Sandbox instance in RUNNING state

        Raises:
            DSBTimeoutError: If sandbox doesn't reach RUNNING state within timeout
            DSBAPIError: If sandbox enters ERROR state
            DSBConnectionError: If connection fails

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     sandbox = await client.sandbox.create_async(image="python:3.12")
            ...     running = await client.sandbox.wait_until_running_async(sandbox.id, timeout=60)
            ...     print(f"Sandbox is running: {running.state}")
        """
        sandbox_id_str = str(sandbox_id)
        start_time = asyncio.get_event_loop().time()
        last_state = None

        while True:
            elapsed = asyncio.get_event_loop().time() - start_time
            if elapsed > timeout:
                from dsb_sdk.exceptions import DSBTimeoutError

                raise DSBTimeoutError(
                    f"Sandbox {sandbox_id_str} did not reach RUNNING state within {timeout}s "
                    f"(last state: {last_state})"
                )

            sandbox = await self.get_async(sandbox_id_str)
            last_state = sandbox.state.value

            if sandbox.state == SandboxState.RUNNING:
                return sandbox

            if sandbox.state == SandboxState.ERROR:
                from dsb_sdk.exceptions import DSBAPIError

                raise DSBAPIError(
                    f"Sandbox {sandbox_id_str} entered ERROR state",
                    status_code=500,
                )

            if sandbox.state == SandboxState.DESTROYING:
                from dsb_sdk.exceptions import DSBAPIError

                raise DSBAPIError(
                    f"Sandbox {sandbox_id_str} is being destroyed",
                    status_code=410,
                )

            await asyncio.sleep(poll_interval)

    async def wait_until_ready_async(
        self,
        sandbox_id: str | UUID,
        timeout: float = 300.0,
        poll_interval: float = 1.0,
    ) -> Sandbox:
        """
        Wait for a sandbox to be fully ready (after RUNNING state).

        This method waits for the sandbox to transition through all creation
        states and be fully operational.

        Args:
            sandbox_id: Sandbox UUID or string
            timeout: Maximum time to wait in seconds (default: 300)
            poll_interval: Polling interval in seconds (default: 1)

        Returns:
            Sandbox instance that is ready

        Raises:
            DSBTimeoutError: If sandbox doesn't become ready within timeout
            DSBAPIError: If sandbox enters ERROR state

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     sandbox = await client.sandbox.create_async(image="python:3.12")
            ...     ready = await client.sandbox.wait_until_ready_async(sandbox.id)
            ...     print(f"Sandbox is ready: {ready.state}")
        """
        # First wait for running state
        await self.wait_until_running_async(sandbox_id, timeout, poll_interval)

        # Allow a small additional delay for full readiness
        await asyncio.sleep(poll_interval)

        # Return the latest state
        return await self.get_async(str(sandbox_id))

    async def cleanup_async(self, sandbox_id: str | UUID) -> dict[str, Any]:
        """
        Force cleanup all resources for a sandbox.

        Args:
            sandbox_id: Sandbox UUID or string

        Returns:
            Cleanup confirmation response

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.sandbox.cleanup_async(sandbox_id)
            ...     print(f"Cleanup status: {result}")
        """
        return await self.transport.request(
            method="POST",
            path=f"/sandboxes/{sandbox_id}/cleanup",
        )

    # Backward compatibility aliases for renamed methods
    async def wait_until_running(
        self, sandbox_id: str | UUID, timeout: float = 300.0, poll_interval: float = 1.0
    ) -> Sandbox:
        """
        Backward compatibility alias for wait_until_running_async.

        Deprecated: Use wait_until_running_async instead.
        """
        return await self.wait_until_running_async(sandbox_id, timeout, poll_interval)

    async def wait_until_ready(
        self, sandbox_id: str | UUID, timeout: float = 300.0, poll_interval: float = 1.0
    ) -> Sandbox:
        """
        Backward compatibility alias for wait_until_ready_async.

        Deprecated: Use wait_until_ready_async instead.
        """
        return await self.wait_until_ready_async(sandbox_id, timeout, poll_interval)
