"""
Synchronous transport implementation using httpx
"""

import json
from collections.abc import Iterator
from typing import Any

import httpx

from dsb_sdk.error_codes import is_validation_error_code
from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBConnectionError,
    DSBTimeoutError,
    DSBValidationError,
)


class SyncTransport:
    """
    Synchronous HTTP transport using httpx.Client.

    Provides HTTP/2 support and connection pooling for sync operations.
    """

    def __init__(
        self,
        api_url: str,
        timeout: float = 30.0,
        verify_ssl: bool = True,
        api_key: str | None = None,
    ):
        """
        Initialize sync transport.

        Args:
            api_url: Base URL for DSB API (e.g., "http://localhost:8080")
            timeout: Request timeout in seconds
            verify_ssl: Whether to verify SSL certificates
            api_key: Optional API key for authentication
        """
        self.api_url = api_url.rstrip("/")
        self.timeout = timeout
        self.verify_ssl = verify_ssl
        self.api_key = api_key
        self._client = httpx.Client(
            base_url=self.api_url,
            timeout=timeout,
            verify=verify_ssl,
            http2=True,
            limits=httpx.Limits(max_keepalive_connections=5, max_connections=10),
        )

    def request(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        json_data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> dict[str, Any]:
        """
        Make a synchronous HTTP request.

        Args:
            method: HTTP method (GET, POST, DELETE, etc.)
            path: API endpoint path (e.g., "/sandboxes")
            params: Query parameters
            json_data: JSON request body
            headers: Additional headers
            timeout: Optional timeout in seconds (overrides default)

        Returns:
            Parsed JSON response

        Raises:
            DSBConnectionError: Connection failure
            DSBAPIError: API error response
            DSBTimeoutError: Request timeout
        """
        request_headers = {"Accept": "application/json"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            response = self._client.request(
                method=method,
                url=path,
                params=params,
                json=json_data,
                headers=request_headers,
                timeout=timeout if timeout is not None else self.timeout,
            )
            response.raise_for_status()

            # Handle empty-body success responses
            if response.status_code == 204:
                return {"deleted": True}
            if response.status_code == 202:
                return {"accepted": True}

            return response.json()

        except httpx.TimeoutException as e:
            raise DSBTimeoutError(f"Request timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            status_code = e.response.status_code
            content_type = e.response.headers.get("Content-Type", "")

            try:
                error_data = e.response.json()

                # Detect RFC 9457 Problem Details format with error_code
                if "error_code" in error_data:
                    error_code = error_data.get("error_code")

                    # Validation errors should raise DSBValidationError for better error handling
                    if is_validation_error_code(error_code):
                        # Extract message from "detail" or "message" field
                        error_msg = error_data.get("detail") or error_data.get("message") or str(error_data)
                        raise DSBValidationError(error_msg) from e

                    # Other error codes use standard DSBAPIError
                    raise DSBAPIError.from_problem_details(error_data) from e

                # Extract error message from various formats
                error_msg = str(error_data)

                # For responses with error_message field (common in backend errors)
                if isinstance(error_data, dict) and "error_message" in error_data:
                    error_msg = error_data.get("error_message", str(error_data))

                # For legacy format with "error" field
                elif "error" in error_data:
                    error_msg = error_data.get("error", str(error_data))

                # Backward compatibility: legacy format
                if "error" in error_data:
                    raise DSBAPIError.from_legacy_format(error_data, status_code) from e

                # Fallback
                raise DSBAPIError(
                    message=error_msg,
                    status_code=status_code,
                    response_data=error_data,
                ) from e

            except json.JSONDecodeError:
                # Non-JSON error
                raise DSBAPIError(
                    message=e.response.text,
                    status_code=status_code,
                ) from e

        except httpx.NetworkError as e:
            raise DSBConnectionError(f"Connection error: {e}") from e

        except httpx.HTTPError as e:
            raise DSBConnectionError(f"HTTP error: {e}") from e

    def stream(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        json_data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> Iterator[dict[str, Any]]:
        """
        Stream SSE events synchronously.

        Args:
            method: HTTP method
            path: API endpoint path
            params: Query parameters
            json_data: JSON request body
            headers: Additional headers

        Yields:
            SSE event data as dictionaries
        """
        request_headers = {"Accept": "text/event-stream"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            with self._client.stream(
                method=method,
                url=path,
                params=params,
                json=json_data,
                headers=request_headers,
                timeout=None,  # No timeout for streaming
            ) as response:
                response.raise_for_status()

                # Parse SSE events
                for line in response.iter_lines():
                    if line.startswith("data: "):
                        data = line[6:]  # Remove "data: " prefix
                        if data.strip() == "[DONE]":
                            break

                        try:
                            yield json.loads(data)
                        except json.JSONDecodeError:
                            continue

        except httpx.HTTPStatusError as e:
            raise DSBAPIError(
                f"Streaming error: {e.response.text}",
                status_code=e.response.status_code,
            ) from e

        except httpx.NetworkError as e:
            raise DSBConnectionError(f"Streaming connection error: {e}") from e

    def upload_multipart(
        self,
        method: str,
        path: str,
        data: dict[str, Any] | None = None,
        files: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> dict[str, Any]:
        """
        Make a multipart/form-data request (for file uploads).

        Args:
            method: HTTP method (POST, PUT, etc.)
            path: API endpoint path
            data: Form data fields
            files: Files to upload. Can be:
                - BinaryIO: File object
                - bytes: Raw bytes
                - tuple: (filename, file_object) or (filename, file_object, content_type)
            headers: Additional headers
            timeout: Optional timeout in seconds (overrides default)

        Returns:
            Parsed JSON response

        Raises:
            DSBConnectionError: Connection failure
            DSBAPIError: API error response
            DSBTimeoutError: Request timeout
        """
        request_headers = {"Accept": "application/json"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            response = self._client.request(
                method=method,
                url=path,
                data=data,
                files=files,
                headers=request_headers,
                timeout=timeout if timeout is not None else self.timeout,
            )
            response.raise_for_status()

            # Handle 204 No Content responses
            if response.status_code == 204:
                return {"uploaded": True}

            return response.json()

        except httpx.TimeoutException as e:
            raise DSBTimeoutError(f"Upload timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            status_code = e.response.status_code
            try:
                error_data = e.response.json()
            except Exception:
                error_data = {"error": e.response.text}
            # Check for 'error' key first (standard API response), then 'message'
            error_msg = error_data.get('error') or error_data.get('message', 'Unknown error')
            raise DSBAPIError(
                f"Upload error: {error_msg}",
                status_code=status_code,
                response_data=error_data,
            ) from e

        except httpx.NetworkError as e:
            raise DSBConnectionError(f"Upload connection error: {e}") from e

        except httpx.HTTPError as e:
            raise DSBConnectionError(f"Upload HTTP error: {e}") from e

    def request_bytes(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> httpx.Response:
        """
        Make an HTTP request and return the raw response object.

        Used for file downloads and other binary data.

        Args:
            method: HTTP method
            path: API endpoint path
            params: Query parameters
            headers: Additional headers
            timeout: Optional timeout in seconds

        Returns:
            httpx.Response object (use .content for bytes, .headers for headers)

        Raises:
            DSBConnectionError: Connection failure
            DSBAPIError: API error response
            DSBTimeoutError: Request timeout
        """
        request_headers = {"Accept": "*/*"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            response = self._client.request(
                method=method,
                url=path,
                params=params,
                headers=request_headers,
                timeout=timeout if timeout is not None else self.timeout,
            )
            response.raise_for_status()
            return response

        except httpx.TimeoutException as e:
            raise DSBTimeoutError(f"Request timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            status_code = e.response.status_code
            try:
                error_data = e.response.json()
            except Exception:
                error_data = {"error": e.response.text}
            # Check for 'error' key first (backend format), then 'message' key (alternative format)
            error_message = error_data.get('error') or error_data.get('message') or str(error_data)
            raise DSBAPIError(
                f"API error: {error_message}",
                status_code=status_code,
                response_data=error_data,
            ) from e

        except httpx.NetworkError as e:
            raise DSBConnectionError(f"Connection error: {e}") from e

        except httpx.HTTPError as e:
            raise DSBConnectionError(f"HTTP error: {e}") from e

    def close(self) -> None:
        """Close the HTTP client."""
        if self._client:
            self._client.close()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()
        return False
