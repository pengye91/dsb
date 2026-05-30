"""
Asynchronous transport implementation using httpx
"""

import json
import time
from collections.abc import AsyncIterator
from typing import Any

import httpx

from dsb_sdk.error_codes import is_validation_error_code
from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBConnectionError,
    DSBTimeoutError,
    DSBValidationError,
)
from dsb_sdk.logging import get_logger

logger = get_logger(__name__)


def _safe_extract_response_text(response, max_length: int = 500) -> str:
    """
    Safely extract text from an HTTP response for logging.

    Handles cases where response might be a Mock object in tests.

    Args:
        response: HTTP response object
        max_length: Maximum length of text to extract

    Returns:
        Extracted text string or error message
    """
    try:
        if hasattr(response, 'text') and isinstance(response.text, str):
            return response.text[:max_length]
        elif hasattr(response, 'content'):
            # Fallback to content bytes
            content = response.content
            if isinstance(content, bytes):
                return content[:max_length].decode('utf-8', errors='ignore')
    except Exception:
        pass
    return "(unable to extract response body)"


class AsyncTransport:
    """
    Asynchronous HTTP transport using httpx.AsyncClient.

    Provides HTTP/2 support, connection pooling, and SSE streaming.
    """

    def __init__(
        self,
        api_url: str,
        timeout: float = 30.0,
        verify_ssl: bool = True,
        api_key: str | None = None,
    ):
        """
        Initialize async transport.

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

        # Log initialization
        logger.info(
            "transport_initialized",
            api_url=self.api_url,
            timeout=timeout,
            verify_ssl=verify_ssl,
            has_api_key=api_key is not None,
        )

        self._client = httpx.AsyncClient(
            base_url=self.api_url,
            timeout=timeout,
            verify=verify_ssl,
            http2=True,
            limits=httpx.Limits(max_keepalive_connections=5, max_connections=10),
        )

    async def request(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        json_data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> dict[str, Any]:
        """
        Make an asynchronous HTTP request with comprehensive logging.

        Args:
            method: HTTP method (GET, POST, DELETE, etc.)
            path: API endpoint path
            params: Query parameters
            json_data: JSON request body
            headers: Additional headers
            timeout: Optional timeout in seconds (uses default if not specified)

        Returns:
            Parsed JSON response

        Raises:
            DSBConnectionError: Connection failure
            DSBAPIError: API error response
            DSBTimeoutError: Request timeout
        """
        start_time = time.time()

        # Log request start (debug level - detailed diagnostics)
        logger.debug(
            "http_request_start",
            method=method,
            path=path,
            has_params=params is not None,
            has_body=json_data is not None,
        )

        request_headers = {"Accept": "application/json"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            # Use provided timeout or default to self.timeout
            request_timeout = timeout if timeout is not None else self.timeout
            response = await self._client.request(
                method=method,
                url=path,
                params=params,
                json=json_data,
                headers=request_headers,
                timeout=request_timeout,
            )
            duration_ms = (time.time() - start_time) * 1000

            # Log success (info level - business event)
            logger.info(
                "http_request_success",
                method=method,
                path=path,
                status_code=response.status_code,
                duration_ms=round(duration_ms, 2),
            )

            response.raise_for_status()

            # Handle empty-body success responses
            if response.status_code == 204:
                return {"deleted": True}
            if response.status_code == 202:
                return {"accepted": True}

            return response.json()

        except httpx.TimeoutException as e:
            duration_ms = (time.time() - start_time) * 1000
            # Log timeout error (error level - requires attention)
            logger.error(
                "http_request_timeout",
                method=method,
                path=path,
                timeout_seconds=timeout or self.timeout,
                duration_ms=round(duration_ms, 2),
                error=str(e),
            )
            raise DSBTimeoutError(f"Request timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            duration_ms = (time.time() - start_time) * 1000
            status_code = e.response.status_code
            content_type = e.response.headers.get("Content-Type", "")

            # Safely extract response body text for logging
            response_body = ""
            try:
                if hasattr(e.response, 'text') and isinstance(e.response.text, str):
                    response_body = e.response.text[:500]
                elif hasattr(e.response, 'content'):
                    # Fallback to content bytes
                    content = e.response.content
                    if isinstance(content, bytes):
                        response_body = content[:500].decode('utf-8', errors='ignore')
            except Exception:
                # If we can't extract the body, just note it
                response_body = "(unable to extract response body)"

            # Log HTTP error (warning level - API returned error but request succeeded)
            logger.warning(
                "http_request_error",
                method=method,
                path=path,
                status_code=status_code,
                response_body=response_body,
                duration_ms=round(duration_ms, 2),
            )

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
            duration_ms = (time.time() - start_time) * 1000
            # Log network error (error level - connection failed)
            logger.error(
                "http_network_error",
                method=method,
                path=path,
                error=str(e),
                duration_ms=round(duration_ms, 2),
            )
            raise DSBConnectionError(f"Connection error: {e}") from e

        except httpx.HTTPError as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "http_generic_error",
                method=method,
                path=path,
                error=str(e),
                duration_ms=round(duration_ms, 2),
            )
            raise DSBConnectionError(f"HTTP error: {e}") from e

    async def stream(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        json_data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """
        Stream SSE events asynchronously with logging.

        Args:
            method: HTTP method
            path: API endpoint path
            params: Query parameters
            json_data: JSON request body
            headers: Additional headers

        Yields:
            SSE event data as dictionaries
        """
        start_time = time.time()

        logger.debug(
            "stream_start",
            method=method,
            path=path,
        )

        request_headers = {"Accept": "text/event-stream"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            async with self._client.stream(
                method=method,
                url=path,
                params=params,
                json=json_data,
                headers=request_headers,
                timeout=None,  # No timeout for streaming
            ) as response:
                duration_ms = (time.time() - start_time) * 1000
                logger.info(
                    "stream_connected",
                    method=method,
                    path=path,
                    status_code=response.status_code,
                    duration_ms=round(duration_ms, 2),
                )
                response.raise_for_status()

                # Parse SSE events
                async for line in response.aiter_lines():
                    if line.startswith("data: "):
                        data = line[6:]  # Remove "data: " prefix
                        if data.strip() == "[DONE]":
                            logger.debug("stream_done", path=path)
                            break

                        try:
                            yield json.loads(data)
                        except json.JSONDecodeError:
                            logger.warning("stream_invalid_json", data=data[:100])
                            continue

        except httpx.HTTPStatusError as e:
            response_text = _safe_extract_response_text(e.response)
            logger.error(
                "stream_error",
                method=method,
                path=path,
                status_code=e.response.status_code,
                error=response_text,
            )
            raise DSBAPIError(
                f"Streaming error: {response_text}",
                status_code=e.response.status_code,
            ) from e

        except httpx.NetworkError as e:
            logger.error(
                "stream_network_error",
                method=method,
                path=path,
                error=str(e),
            )
            raise DSBConnectionError(f"Streaming connection error: {e}") from e

    async def upload_multipart(
        self,
        method: str,
        path: str,
        data: dict[str, Any] | None = None,
        files: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """
        Make an asynchronous multipart/form-data request (for file uploads) with logging.

        Args:
            method: HTTP method (POST, PUT, etc.)
            path: API endpoint path
            data: Form data fields
            files: Files to upload. Can be:
                - BinaryIO: File object
                - bytes: Raw bytes
                - tuple: (filename, file_object) or (filename, file_object, content_type)
            headers: Additional headers

        Returns:
            Parsed JSON response

        Raises:
            DSBConnectionError: Connection failure
            DSBAPIError: API error response
            DSBTimeoutError: Request timeout
        """
        start_time = time.time()

        logger.debug(
            "upload_start",
            method=method,
            path=path,
            has_data=data is not None,
            has_files=files is not None,
        )

        request_headers = {"Accept": "application/json"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            response = await self._client.request(
                method=method,
                url=path,
                data=data,
                files=files,
                headers=request_headers,
            )
            duration_ms = (time.time() - start_time) * 1000

            logger.info(
                "upload_success",
                method=method,
                path=path,
                status_code=response.status_code,
                duration_ms=round(duration_ms, 2),
            )

            response.raise_for_status()

            # Handle 204 No Content responses
            if response.status_code == 204:
                return {"uploaded": True}

            return response.json()

        except httpx.TimeoutException as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "upload_timeout",
                method=method,
                path=path,
                duration_ms=round(duration_ms, 2),
                error=str(e),
            )
            raise DSBTimeoutError(f"Upload timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            duration_ms = (time.time() - start_time) * 1000
            status_code = e.response.status_code

            logger.warning(
                "upload_error",
                method=method,
                path=path,
                status_code=status_code,
                duration_ms=round(duration_ms, 2),
                response_body=_safe_extract_response_text(e.response),
            )

            try:
                error_data = e.response.json()
            except Exception:
                error_data = {"error": _safe_extract_response_text(e.response, max_length=1000)}
            # Check for 'error' key first (standard API response), then 'message'
            error_msg = error_data.get('error') or error_data.get('message', 'Unknown error')
            raise DSBAPIError(
                f"Upload error: {error_msg}",
                status_code=status_code,
                response_data=error_data,
            ) from e

        except httpx.NetworkError as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "upload_network_error",
                method=method,
                path=path,
                error=str(e),
                duration_ms=round(duration_ms, 2),
            )
            raise DSBConnectionError(f"Upload connection error: {e}") from e

        except httpx.HTTPError as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "upload_generic_error",
                method=method,
                path=path,
                error=str(e),
                duration_ms=round(duration_ms, 2),
            )
            raise DSBConnectionError(f"Upload HTTP error: {e}") from e

    async def request_bytes_async(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> httpx.Response:
        """
        Make an asynchronous HTTP request and return the raw response object with logging.

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
        start_time = time.time()

        logger.debug(
            "bytes_request_start",
            method=method,
            path=path,
        )

        request_headers = {"Accept": "*/*"}
        if headers:
            request_headers.update(headers)
        # Add API key header if configured
        if self.api_key:
            request_headers["X-API-Key"] = self.api_key

        try:
            response = await self._client.request(
                method=method,
                url=path,
                params=params,
                headers=request_headers,
                timeout=timeout if timeout is not None else self.timeout,
            )
            duration_ms = (time.time() - start_time) * 1000

            logger.info(
                "bytes_request_success",
                method=method,
                path=path,
                status_code=response.status_code,
                content_length=response.headers.get("content-length"),
                duration_ms=round(duration_ms, 2),
            )

            response.raise_for_status()
            return response

        except httpx.TimeoutException as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "bytes_request_timeout",
                method=method,
                path=path,
                duration_ms=round(duration_ms, 2),
                error=str(e),
            )
            raise DSBTimeoutError(f"Request timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            duration_ms = (time.time() - start_time) * 1000
            status_code = e.response.status_code

            logger.warning(
                "bytes_request_error",
                method=method,
                path=path,
                status_code=status_code,
                duration_ms=round(duration_ms, 2),
                response_body=_safe_extract_response_text(e.response),
            )

            try:
                error_data = e.response.json()
            except Exception:
                error_data = {"message": _safe_extract_response_text(e.response, max_length=1000)}
            raise DSBAPIError(
                f"API error: {error_data.get('message', 'Unknown error')}",
                status_code=status_code,
                response_data=error_data,
            ) from e

        except httpx.NetworkError as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "bytes_request_network_error",
                method=method,
                path=path,
                error=str(e),
                duration_ms=round(duration_ms, 2),
            )
            raise DSBConnectionError(f"Connection error: {e}") from e

        except httpx.HTTPError as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.error(
                "bytes_request_generic_error",
                method=method,
                path=path,
                error=str(e),
                duration_ms=round(duration_ms, 2),
            )
            raise DSBConnectionError(f"HTTP error: {e}") from e

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client:
            await self._client.aclose()

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()
        return False
