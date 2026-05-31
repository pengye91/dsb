"""
Images API implementation (synchronous)

Provides synchronous methods for managing Docker images.
Use with DSBClient.
"""

from __future__ import annotations

from collections.abc import Iterator
from typing import Any
from urllib.parse import quote

from dsb_sdk.transport.sync import SyncTransport


class ImagesAPI:
    """
    API for managing Docker images (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize images API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport

    def list(self) -> list[dict[str, Any]]:
        """
        List all available images.

        Note: Image management endpoints are currently only fully supported on the
        Docker backend. For Kubernetes backend, this will return the images
        managed by the backend daemon or return a 501 Not Implemented error.

        Returns:
            List of ImageSummary dicts with keys: id, repo_tags, size, created, labels
        """
        return self.transport.request(
            method="GET",
            path="/images",
        )

    def get(self, image_id: str) -> dict[str, Any]:
        """
        Get detailed information about a specific image.

        Note: Image management endpoints are currently only fully supported on the
        Docker backend. For Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Args:
            image_id: Image identifier (e.g. "sha256:abc...")

        Returns:
            ImageDetails dict with extended fields like architecture, os, env, features
        """
        encoded_id = quote(image_id, safe="")
        return self.transport.request(
            method="GET",
            path=f"/images/{encoded_id}",
        )

    def pull(self, image: str, tag: str | None = None) -> None:
        """
        Pull an image (non-streaming).

        Sends a pull request and returns when the server acknowledges it (202 Accepted).

        Note: This is generally only supported by the Docker backend. For the
        Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Args:
            image: Image name (e.g. "ubuntu")
            tag: Image tag (default: "latest")
        """
        self.transport.request(
            method="POST",
            path="/images/pull",
            json_data={"image": image, "tag": tag or "latest"},
        )

    def pull_stream(
        self, image: str, tag: str | None = None
    ) -> Iterator[dict[str, Any]]:
        """
        Pull an image with streaming progress events via SSE.

        Note: This is generally only supported by the Docker backend. For the
        Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Args:
            image: Image name (e.g. "ubuntu")
            tag: Image tag (default: "latest")

        Yields:
            PullProgressEvent dicts from the SSE stream
        """
        yield from self.transport.stream(
            method="POST",
            path="/images/pull-stream",
            json_data={"image": image, "tag": tag or "latest"},
        )

    def delete(self, image_id: str) -> None:
        """
        Delete an image.

        Note: This is generally only supported by the Docker backend. For the
        Kubernetes backend, this might raise a NOT_SUPPORTED error.

        Args:
            image_id: Image identifier (e.g. "sha256:abc...")
        """
        encoded_id = quote(image_id, safe="")
        self.transport.request(
            method="DELETE",
            path=f"/images/{encoded_id}",
        )
