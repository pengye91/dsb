"""
Images API implementation (asynchronous)

Provides asynchronous methods for managing Docker images.
Use with AsyncDSBClient.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Any
from urllib.parse import quote

from dsb_sdk.transport.async_transport import AsyncTransport


class AsyncImagesAPI:
    """
    API for managing Docker images (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async images API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def list_async(self) -> list[dict[str, Any]]:
        """
        List all available images.

        Returns:
            List of ImageSummary dicts with keys: id, repo_tags, size, created, labels

        Note: Image management endpoints are currently only fully supported on the
        Docker backend. For Kubernetes backend, this will return the images
        managed by the backend daemon or return a 501 Not Implemented error.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     images = await client.images.list_async()
        """
        return await self.transport.request(
            method="GET",
            path="/images",
        )

    async def get_async(self, image_id: str) -> dict[str, Any]:
        """
        Get detailed information about a specific image.

        Args:
            image_id: Image identifier (e.g. "sha256:abc...")

        Returns:
            ImageDetails dict with extended fields like architecture, os, env, features

        Note: Image management endpoints are currently only fully supported on the
        Docker backend. For Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     details = await client.images.get_async(image_id)
        """
        encoded_id = quote(image_id, safe="")
        return await self.transport.request(
            method="GET",
            path=f"/images/{encoded_id}",
        )

    async def pull_async(self, image: str, tag: str | None = None) -> None:
        """
        Pull an image (non-streaming).

        Sends a pull request and returns when the server acknowledges it (202 Accepted).

        Args:
            image: Image name (e.g. "ubuntu")
            tag: Image tag (default: "latest")

        Note: This is generally only supported by the Docker backend. For the
        Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     await client.images.pull_async("ubuntu", tag="22.04")
        """
        await self.transport.request(
            method="POST",
            path="/images/pull",
            json_data={"image": image, "tag": tag or "latest"},
        )

    async def pull_stream_async(
        self, image: str, tag: str | None = None
    ) -> AsyncIterator[dict[str, Any]]:
        """
        Pull an image with streaming progress events via SSE.

        Args:
            image: Image name (e.g. "ubuntu")
            tag: Image tag (default: "latest")

        Yields:
            PullProgressEvent dicts from the SSE stream

        Note: This is generally only supported by the Docker backend. For the
        Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     async for event in await client.images.pull_stream_async("ubuntu"):
            ...         print(event)
        """
        return self.transport.stream(
            method="POST",
            path="/images/pull-stream",
            json_data={"image": image, "tag": tag or "latest"},
        )

    async def delete_async(self, image_id: str) -> None:
        """
        Delete an image.

        Args:
            image_id: Image identifier (e.g. "sha256:abc...")

        Note: This is generally only supported by the Docker backend. For the
        Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     await client.images.delete_async(image_id)
        """
        encoded_id = quote(image_id, safe="")
        await self.transport.request(
            method="DELETE",
            path=f"/images/{encoded_id}",
        )
