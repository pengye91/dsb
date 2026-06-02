# Changelog

All notable changes to the DSB Python SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_Nothing yet — entries will be added here as future work lands._

## [0.2.0] - 2026-06-02

### Added

#### Databend Database API

- **Databend API** (`DatabendAPI` and `AsyncDatabendAPI`)
  - `execute_sql(sandbox_id, sql, limit=-1)` - Execute SQL queries with optional row limit
  - `list_tables(sandbox_id, databases)` - List tables in specified databases (supports virtual databases)
  - `describe_table(sandbox_id, table)` - Get table schema with column details
  - `download_to_json(sandbox_id, sql, output)` - Export query results to JSON file
  - `convert_to_js(sandbox_id, json_file, js_var, output)` - Convert JSON to JavaScript variable
  - **Tool Path**: `/opt/tools/databend_tools.py` (renamed from `/opt/browser_tools/`)
  - **New Types** in `dsb_sdk.types.sandbox`:
    - `DatabendConfig` - Database configuration with credential management
    - `to_environment_dict()` - Convert config to environment variables for auto-injection
  - **Client Integration**: `databend` property added to `DSBClient` and `AsyncDSBClient`
  - **Credential Auto-Injection**: Databend credentials automatically injected as environment variables when creating sandboxes
  - **Full Async Support**: All Databend operations available in async API
  - **Virtual Database Support**: Compatible with XML schema-based virtual databases (compliance_virtual_cluster)
  - **Generic Types**: Uses `Dict[str, Any]` for flexible responses (no per-command Pydantic models)

- **DatabendConfig** in `SandboxConfig` for automatic credential injection
  - Add `databend: Optional[DatabendConfig]` parameter to `sandbox.create()` and `sandbox.create_async()`
  - Automatically injects `DATABEND_HOST`, `DATABEND_PORT`, `DATABEND_USER`, `DATABEND_PASSWORD`, `DATABEND_DATABASE` as environment variables
  - Optional configuration: `virtual_db_prefix` and `meta_path` for virtual database support

#### Static File Serving

- **Static Files API** (`StaticFilesAPI` and `AsyncStaticFilesAPI`)
  - `serve_file(sandbox_id, file_path)` - Serve static files as binary content
  - `list_files(sandbox_id)` - List all published files with metadata
  - `delete_file(sandbox_id, file_path)` - Delete a specific file
  - `delete_sandbox_files(sandbox_id)` - Delete all files for a sandbox
- **New Types** in `dsb_sdk.types.sandbox`:
  - `StaticFileMetadata` - Metadata for individual files (name, path, size, content_type)
  - `StaticFileList` - List of files with metadata (sandbox_id, files, total_count, total_size_bytes)
- **Client Integration**: `static_files` property added to `DSBClient` and `AsyncDSBClient`
- **Binary File Support**: Direct HTTPX client integration for serving binary content
- **Full Async Support**: All static file operations available in async API

#### Production Infrastructure

- **Makefile** with comprehensive commands for development, testing, and quality assurance
- **Structured Logging** (`dsb_sdk.logging`) with JSON format support and context tracking
- **Metrics Collection** (`dsb_sdk.metrics`) with Prometheus integration
- **Retry Logic** (`dsb_sdk.utils.retry`) with exponential backoff
- **Circuit Breaker Pattern** (`dsb_sdk.utils.circuit`) for preventing cascading failures
- **Error Suggestions** helper function in exceptions module

#### Enhanced Error Handling

- `is_retryable_error()` function to check if errors should be retried
- `get_error_suggestion()` function to get helpful error recovery suggestions
- Retryable classification on all DSB exceptions
- New exception types:
  - `DSBCircuitOpenError` - Circuit breaker is open
  - `DSBAuthenticationError` - Authentication failures
  - `DSBRateLimitError` - Rate limit exceeded with retry_after support

#### Feature Profiles in SandboxConfig

- `features` parameter: Enable specific features from Docker image labels
- `enable_all_features` parameter: Enable all default features from image
- Auto-configuration of: ports, volumes, environment variables, commands, static server settings
- Backward compatible: Optional parameters with sensible defaults

#### Development Tools

- Security scanning with `bandit` and `safety`
- Dependency auditing with `pip-audit`
- Performance benchmarking with `pytest-benchmark`
- Coverage reporting with `pytest-cov`
- Linting and formatting with `ruff`
- Type checking with `mypy`

#### Documentation

- **Feature Profiles Section** in README.md
  - Usage examples for `features` and `enable_all_features`
  - Explanation of how feature profiles work
  - Link to comprehensive feature profiles documentation
- **Static File Serving Section** in README.md
  - Complete usage examples (sync and async)
  - Use cases and supported file types
  - API reference for all static files methods
  - Type documentation for StaticFileMetadata and StaticFileList
- **Updated API Reference**:
  - Added `static_files` property to client properties
  - Added Static Files API section with method signatures
  - Documented return types and metadata structure

### Changed

- **Tool Directory**: `/opt/browser_tools/` → `/opt/tools/`
  - `WEB_TOOLS_PATH`: `/opt/tools/web_tools.py`
  - `BROWSER_TOOLS_PATH`: `/opt/tools/browser_tools.js`
  - Backward compatible: old paths still work during migration
- **Web Tools Path**: Updated from `/opt/browser_tools/` to `/opt/tools/`
- **Browser Tools Path**: Updated from `/opt/browser_tools/` to `/opt/tools/`
- **Test Assertions**: Updated test expectations to use new `/opt/tools/` path
- Enhanced exception hierarchy with retryable flags
- Improved error messages with contextual suggestions
- Updated README with troubleshooting section

### Dependencies

- `tenacity>=8.5.0` - Retry logic with exponential backoff
- `pybreaker>=1.0.0` - Circuit breaker implementation
- `structlog>=24.0.0` - Structured logging
- `prometheus-client>=0.20.0` - Metrics collection
- `pytest-benchmark>=4.0.0` - Performance benchmarking (dev)
- `bandit>=1.7.0` - Security linting (dev)
- `pip-audit>=2.7.0` - Dependency auditing (dev)
- `safety>=3.0.0` - Security vulnerability checking (dev)

### Security

- Added security audit tools and documentation
- Documented authentication best practices

## [0.1.0] - 2026-01-08

### Added

#### Core SDK Features

- Synchronous `DSBClient` and asynchronous `AsyncDSBClient`
- **Sandbox Management**
  - Create, list, get, stop, delete sandboxes
  - Execute commands in sandboxes
  - Stream creation progress via SSE
  - Get resource statistics

- **SSH Session Management**
  - Create SSH sessions with tracking
  - Heartbeat mechanism for session monitoring
  - Session termination and cleanup

- **Interactive Terminals**
  - WebSocket-based terminal sessions
  - Real-time command execution
  - Context manager support for automatic cleanup

- **Web Scraping & Browser Automation**
  - Web page scraping (markdown, HTML, text, links)
  - CSS selector-based data extraction
  - Table extraction from HTML
  - Screenshot capture
  - Web search across multiple engines (Google, DuckDuckGo, Bing, Baidu)
  - Link discovery and crawling
  - Browser automation (navigate, click, fill, evaluate JavaScript)

- **Activity Monitoring**
  - List all activities
  - Cleanup inactive sandboxes
  - Health status checks

#### Architecture

- Clean separation between API modules and transport layer
- Pydantic v2 models for type safety and validation
- Full type hints with MyPy strict mode
- Minimal dependency footprint (6 core packages)

#### Documentation

- Comprehensive README with quick start guide
- API reference for all modules
- Usage examples for synchronous and asynchronous APIs
- Error handling examples
- Development setup guide

#### Testing

- 344 tests (unit + integration)
- Test fixtures for mocking and live testing
- Pytest markers for test categorization
- Coverage reporting

## [0.0.1] - Initial Release

### Added

- Initial SDK release with basic sandbox management
- Synchronous and asynchronous client support
- Basic error handling

---

## Release Checklist

Before releasing a new version, ensure:

- [ ] All tests pass (`make test-all`)
- [ ] Code coverage is >80% (`make test-cov`)
- [ ] Linting passes (`make lint`)
- [ ] Type checking passes (`make type-check`)
- [ ] Security audit passes (`make security-full`)
- [ ] Dependencies are up-to-date (`make deps-audit`)
- [ ] Documentation is updated
- [ ] CHANGELOG.md is updated
- [ ] Version number is updated in `pyproject.toml`
- [ ] Release notes are prepared

## Version Strategy

- **Major version (X.0.0)**: Breaking changes, requires migration
- **Minor version (0.X.0)**: New features, backward compatible
- **Patch version (0.0.X)**: Bug fixes, backward compatible

## Deprecation Policy

Features will be deprecated for at least one minor version before removal.
Deprecated features will emit warnings when used.

Example:

- v0.2.0: Feature X is deprecated (warning)
- v0.3.0: Feature X still works but is deprecated (warning)
- v0.4.0: Feature X is removed (breaking change)

---

[Unreleased]: https://github.com/xieyuanpeng/dsb/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/xieyuanpeng/dsb/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/xieyuanpeng/dsb/releases/tag/v0.1.0
[0.0.1]: https://github.com/xieyuanpeng/dsb/releases/tag/v0.0.1
