# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial open source release preparation
- Comprehensive test suite with unit and integration tests
- Docker-based sandbox management system
- WebSocket-based real-time log streaming
- SSH gateway for container access
- Dashboard for sandbox management and monitoring
- MCP (Model Context Protocol) server for AI agent integration
- Python SDK for programmatic access
- Unified error code system across all components

### Changed

- Migrated from Puppeteer to Playwright for browser automation
- Improved configuration system with environment-based overrides
- Enhanced Docker image management with feature profiles

### Fixed

- Clippy warnings and compilation errors
- Database connection pool management
- Container lifecycle edge cases

## [0.1.0] - 2026-04-28

### Added

- Initial release of DSB (Docker Sandbox)
- Core sandbox management functionality
- Docker container lifecycle management
- Basic CLI interface
- REST API for sandbox operations
- PostgreSQL persistence layer
- Activity logging system
- Session token management

### Security

- Implemented secure container isolation
- Added authentication and authorization layer
- Implemented proper secret management

---

For the complete list of changes, please see the [commit history](https://github.com/pengye91/dsb/commits/main).
