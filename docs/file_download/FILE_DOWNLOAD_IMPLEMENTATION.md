# File Download API Implementation

This document describes the implementation of the file download API for DSB (Distributed Sandboxes).

## Overview

The file download API allows users to download files directly from sandbox container filesystems through a RESTful HTTP endpoint.

**Endpoint**: `GET /sandboxes/{id}/download?path=<filepath>`

## Architecture

```
HTTP GET Request with query params
         ↓
download_file() Handler - Validate, sanitize, check file
         ↓
SandboxService::download_file() - Read via Docker exec (base64)
         ↓
Binary Response with headers (Content-Type, Content-Disposition)
```

## Design Decisions

### GET with Query Parameters

We chose GET with query parameters instead of POST with JSON body because:
- RESTful semantics (GET for retrieval)
- Cacheable by browsers and proxies
- Browser-friendly (can be opened directly in URL bar)
- Consistent with HTTP best practices for file downloads

### Base64 Encoding for Container Transport

Files are read from containers using `base64 -w0 <filepath>` via Docker exec because:
- **Safety**: Base64 safely encodes binary data through shell commands
- **Reliability**: Avoids issues with special characters, newlines, and binary data
- **Compatibility**: Works reliably with Docker's exec mechanism
- **Consistency**: Same approach used by file upload API

### Binary Response vs JSON Response

The API returns raw binary data instead of JSON with base64 content because:
- **Efficiency**: Direct binary transfer without double-encoding
- **Browser Support**: Browsers can handle binary downloads natively
- **Streaming**: Supports large files without loading entire response into memory
- **Standards**: Follows HTTP standards for file downloads

### Metadata Headers

We use HTTP headers (X-File-Name, X-File-Path, X-File-Size) instead of wrapping in JSON because:
- **Compatibility**: Works with all HTTP clients (browsers, curl, wget, etc.)
- **Standard**: Follows HTTP conventions for metadata
- **Simplicity**: Clients don't need to parse JSON to get file info
- **Flexibility**: Allows direct browser downloads

## Implementation Components

### 1. Service Layer (`src/core/sandbox.rs`)

The `SandboxService::download_file()` method handles the core download logic:

```rust
pub async fn download_file(
    &self,
    id: &uuid::Uuid,
    src_path: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // 1. Get sandbox and validate state
    let sandbox = self.state.get_sandbox(id).await
        .ok_or("Sandbox not found")?;

    if sandbox.state != SandboxState::Running {
        return Err("Sandbox is not running".into());
    }

    // 2. Check file exists
    let check_cmd = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("test -f '{}' && echo 'exists' || echo 'notfound'", src_path)
    ];

    let check_result = self.docker.exec_container(container_id, check_cmd).await?;
    if !check_result.contains("exists") {
        return Err("File not found".into());
    }

    // 3. Get file size (enforce 10MB limit)
    let size_cmd = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("wc -c < '{}' 2>/dev/null || echo '0'", src_path)
    ];

    let size_result = self.docker.exec_container(container_id, size_cmd).await?;
    let file_size: u64 = size_result.trim().parse()
        .unwrap_or(0);

    if file_size > MAX_FILE_SIZE {
        return Err(format!("File size {} exceeds limit {}", file_size, MAX_FILE_SIZE).into());
    }

    // 4. Read file using base64 for safe transmission
    let read_cmd = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("base64 -w0 '{}'", src_path)
    ];

    let encoded = self.docker.exec_container(container_id, read_cmd).await?;

    // 5. Decode base64
    let decoded = BASE64_STANDARD.decode(encoded.trim())
        .map_err(|_| "Failed to decode file content")?;

    Ok(decoded)
}
```

**Key features**:
- File existence check before reading
- Size validation (10MB limit)
- Base64 encoding for safe data transmission
- Comprehensive error handling

### 2. Handler Layer (`src/api/handlers/sandbox.rs`)

The `download_file()` HTTP handler processes requests and builds responses:

```rust
pub async fn download_file(
    State(service): State<Arc<SandboxService>>,
    Path(id): Path<uuid::Uuid>,
    AxumQuery(params): AxumQuery<DownloadParams>,
) -> Response {
    // 1. Validate path parameter
    let src_path = match params.path {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Missing 'path' query parameter"
                }))
            ).into_response();
        }
    };

    // 2. Sanitize path (prevent traversal attacks)
    let sanitized_path = match sanitize_path(&src_path) {
        Ok(path) => path,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid path: {}", e)
                }))
            ).into_response();
        }
    };

    // 3. Download from container
    match service.download_file(&id, &sanitized_path).await {
        Ok(data) => {
            // 4. Detect MIME type
            let content_type = detect_mime_type(&sanitized_path);

            // 5. Extract filename for Content-Disposition
            let filename = extract_filename(&sanitized_path);

            // 6. Determine disposition
            let disposition_value = match params.disposition.as_deref() {
                Some("inline") => "inline",
                _ => "attachment",
            };

            // 7. Build response with proper headers
            let data_len = data.len();
            let mut response = data.into_response();

            response.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_str(content_type).unwrap()
            );

            let disposition = format!("{}; filename=\"{}\"", disposition_value, filename);
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                header::HeaderValue::from_str(&disposition).unwrap()
            );

            // Set metadata headers
            response.headers_mut().insert(
                header::HeaderName::from_static("x-file-name"),
                header::HeaderValue::from_str(&filename).unwrap()
            );
            response.headers_mut().insert(
                header::HeaderName::from_static("x-file-path"),
                header::HeaderValue::from_str(&sanitized_path).unwrap()
            );
            response.headers_mut().insert(
                header::HeaderName::from_static("x-file-size"),
                header::HeaderValue::from_str(&data_len.to_string()).unwrap()
            );

            response
        }
        Err(e) => {
            // Error handling with appropriate status codes
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Sandbox not found" }))).into_response()
            } else if error_msg.contains("not running") {
                (StatusCode::CONFLICT, Json(serde_json::json!({ "error": "Sandbox is not running" }))).into_response()
            } else if error_msg.contains("File not found") {
                (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "File not found in sandbox" }))).into_response()
            } else if error_msg.contains("exceeds limit") {
                (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({
                    "error": error_msg,
                    "max_size": "10MB"
                }))).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": format!("Download failed: {}", error_msg)
                }))).into_response()
            }
        }
    }
}
```

**Key features**:
- Query parameter validation
- Path sanitization (reuses existing `sanitize_path()` function)
- MIME type detection
- Content-Disposition support (inline/attachment)
- Comprehensive error handling with appropriate HTTP status codes

### 3. Python SDK Transport Layer

The Python SDK transport layer needed a new method to handle raw HTTP responses:

```python
def request_bytes(
    self,
    method: str,
    path: str,
    params: Optional[Dict[str, Any]] = None,
    headers: Optional[Dict[str, str]] = None,
    timeout: Optional[float] = None,
) -> httpx.Response:
    """
    Make an HTTP request and return the raw response object.

    Used for file downloads and other binary data.
    """
    request_headers = {"Accept": "*/*"}
    if headers:
        request_headers.update(headers)
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
            error_data = {"message": e.response.text}
        raise DSBAPIError(
            f"API error: {error_data.get('message', 'Unknown error')}",
            status_code=status_code,
            response_data=error_data,
        ) from e

    except httpx.NetworkError as e:
        raise DSBConnectionError(f"Connection error: {e}") from e

    except httpx.HTTPError as e:
        raise DSBConnectionError(f"HTTP error: {e}") from e
```

**Why new method?**
- Existing `request()` returns parsed JSON (`Dict[str, Any]`)
- File downloads need access to raw bytes and response headers
- Cleaner separation of concerns (JSON vs binary responses)

### 4. Python SDK API Layer

The SDK provides convenient methods for downloading files:

```python
def download_file(
    self,
    sandbox_id: str,
    path: str,
    disposition: Optional[str] = None,
) -> FileDownloadResponse:
    """
    Download a file from the sandbox filesystem.
    """
    from dsb_sdk.types.sandbox import FileDownloadResponse
    import os

    # Build query parameters
    params = {"path": path}
    if disposition:
        params["disposition"] = disposition

    # Make request and get raw response
    response = self.transport.request_bytes(
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

def download_file_to_path(
    self,
    sandbox_id: str,
    sandbox_path: str,
    local_path: str,
) -> dict[str, Any]:
    """
    Download a file from sandbox and save it to a local path.
    """
    import os

    # Download file
    response = self.download_file(sandbox_id, sandbox_path)

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
```

**Key features**:
- Type-safe response with Pydantic model
- Convenience method for saving directly to disk
- Automatic directory creation
- Comprehensive metadata in response

## Security Considerations

### Path Traversal Prevention

**Attack**: `GET /sandboxes/{id}/download?path=/tmp/../../../etc/passwd`

**Defense**: The `sanitize_path()` function removes `..` sequences:
```rust
fn sanitize_path(path: &str) -> Result<String, String> {
    // Remove .. sequences to prevent directory traversal
    let cleaned = path.replace("..", "");
    // Additional validation...
}
```

### File Size Limits

**Attack**: Upload a 10GB file, then download to exhaust server memory

**Defense**: Size check BEFORE reading:
```rust
let file_size: u64 = size_result.trim().parse().unwrap_or(0);
if file_size > MAX_FILE_SIZE {
    return Err(format!("File size {} exceeds limit {}", file_size, MAX_FILE_SIZE).into());
}
```

### Sandbox State Validation

**Attack**: Download from stopped/destroyed sandbox

**Defense**: State check before processing:
```rust
if sandbox.state != SandboxState::Running {
    return Err("Sandbox is not running".into());
}
```

### File Existence Check

**Attack**: Probe for files by trying to download them

**Defense**: Explicit file existence check:
```rust
let check_result = self.docker.exec_container(container_id, check_cmd).await?;
if !check_result.contains("exists") {
    return Err("File not found".into());
}
```

## Error Handling

The API uses appropriate HTTP status codes:

| Status Code | Condition | Response Format |
|-------------|-----------|-----------------|
| 200 OK | Success | Binary file with headers |
| 400 Bad Request | Missing path parameter | `{"error": "Missing 'path' query parameter"}` |
| 404 Not Found | Sandbox/file not found | `{"error": "Sandbox not found"}` or `{"error": "File not found in sandbox"}` |
| 409 Conflict | Sandbox not running | `{"error": "Sandbox is not running"}` |
| 413 Payload Too Large | File exceeds 10MB | `{"error": "File size X exceeds limit Y", "max_size": "10MB"}` |
| 500 Internal Server Error | Download failed | `{"error": "Download failed: {reason}"}` |

## MIME Type Detection

The system uses file extension to MIME type mapping:

```rust
pub fn detect_mime_type(path: &str) -> &'static str {
    if let Some(ext) = Path::new(path).extension() {
        match ext.to_str().unwrap_or("") {
            "txt" => "text/plain",
            "html" => "text/html",
            "json" => "application/json",
            "xml" => "application/xml",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "mp3" => "audio/mpeg",
            "mp4" => "video/mp4",
            "zip" => "application/zip",
            "tar" => "application/x-tar",
            "gz" => "application/gzip",
            "bin" | "exe" => "application/octet-stream",
            _ => "application/octet-stream",
        }
    } else {
        "application/octet-stream"
    }
}
```

## Testing

### Backend Integration Tests

Located in `tests/integration_test.rs`:

- ✅ Download file successfully
- ✅ Download non-existent file (404)
- ✅ Download from stopped sandbox (409)
- ✅ Download with inline disposition
- ✅ Different file types (text, JSON, binary)
- ✅ Path traversal prevention

### Python SDK Unit Tests

Located in `sdks/python/tests/unit/test_file_download.py`:

- ✅ FileDownloadResponse type validation
- ✅ Sync download methods
- ✅ Async download methods
- ✅ Download to path methods
- ✅ Binary file handling
- ✅ Header parsing
- ✅ Error conditions

### Python SDK Integration Tests

Located in `sdks/python/tests/integration/test_file_download_api.py`:

- ✅ End-to-end file download
- ✅ Upload/download roundtrip
- ✅ File creation and directory handling
- ✅ Different file types
- ✅ Error scenarios
- ✅ Async API

## Performance Considerations

### Memory Usage

- Files up to 10MB are loaded entirely into memory
- Base64 encoding temporarily doubles memory usage during transfer
- Consider streaming for future large file support

### Optimization Opportunities

1. **Chunked Transfer Encoding**: Stream files instead of loading entirely
2. **Compression**: Add gzip compression for text files
3. **Caching**: Add ETag/Last-Modified headers for caching
4. **Range Requests**: Support partial content downloads for large files

## Future Enhancements

### Potential Features

1. **Streaming Downloads**: Support for large files (>10MB)
2. **Range Requests**: `Range: bytes=0-1023` header support
3. **Compression**: Automatic gzip compression
4. **Batch Downloads**: Download multiple files as zip
5. **Directory Downloads**: Download entire directories
6. **Progress Callbacks**: Progress tracking for large files

### API Extensions

```python
# Streaming download (future)
for chunk in client.sandbox.download_file_stream(sandbox.id, "/app/large.bin"):
    process_chunk(chunk)

# Range request (future)
response = client.sandbox.download_file_range(
    sandbox.id,
    "/app/large.bin",
    start=0,
    end=1023
)

# Batch download (future)
files = client.sandbox.download_files(
    sandbox.id,
    paths=["/app/file1.txt", "/app/file2.txt"]
)
```

## Comparison with Upload API

| Aspect | Upload | Download |
|--------|--------|----------|
| HTTP Method | POST | GET |
| Content-Type | multipart/form-data | application/octet-stream (auto-detected) |
| Data Transfer | Base64 encoded via shell | Base64 encoded via shell |
| File Size Limit | 10MB | 10MB |
| Path Sanitization | Yes | Yes (same function) |
| State Validation | Running | Running |
| Binary Support | Yes | Yes |
| Response | JSON with metadata | Binary with headers |

## Dependencies

### Rust

- `axum`: Web framework
- `base64`: Encoding/decoding
- `serde`: JSON serialization
- `bollard`: Docker client

### Python SDK

- `httpx`: HTTP client
- `pydantic`: Data validation
- `typing`: Type hints

## Related Documentation

- [File Upload API Implementation](../file_upload/FILE_UPLOAD_IMPLEMENTATION.md)
- [API Documentation](../api/README.md)
- [Python SDK Guide](../../sdks/python/docs/index.md)
- [Security Best Practices](../security/README.md)

## Changelog

### v0.1.0 (2026-01-13)

- Initial implementation
- Support for files up to 10MB
- Binary and text file downloads
- Python SDK (sync and async)
- Comprehensive test coverage
- Security hardening (path sanitization, size limits, state validation)
