"""
Test Fixtures Package

This package contains shared fixtures for DSB SDK tests:
- model_fixtures: Factory functions for Pydantic models
- api_responses: Real API response JSON fixtures
"""

from tests.fixtures.model_fixtures import (
    create_browser_action_response,
    # Web fixtures
    create_browser_info,
    create_browser_tab_info,
    create_databend_config,
    create_exec_request,
    # Exec fixtures
    create_exec_response,
    create_file_download_response,
    create_file_info,
    create_pagination_meta,
    # Config fixtures
    create_resource_limits,
    # Sandbox fixtures
    create_sandbox,
    create_sandbox_config,
    create_sandbox_create_request,
    create_sandbox_list_response,
    create_sandbox_progress_event,
    create_sandbox_stats,
    # SSH fixtures
    create_ssh_session,
    create_ssh_session_config,
    create_static_file_list,
    create_static_file_metadata,
    create_ulimit,
    create_upload_file_response,
    create_web_crawl_response,
    create_web_crawl_result,
    create_web_health_response,
    create_web_scrape_result,
)

__all__ = [
    # Sandbox
    "create_sandbox",
    "create_sandbox_config",
    "create_sandbox_stats",
    "create_sandbox_create_request",
    "create_sandbox_progress_event",
    "create_sandbox_list_response",
    "create_pagination_meta",
    "create_static_file_metadata",
    "create_static_file_list",
    "create_file_info",
    "create_upload_file_response",
    "create_file_download_response",
    # Config
    "create_resource_limits",
    "create_databend_config",
    "create_ulimit",
    # SSH
    "create_ssh_session",
    "create_ssh_session_config",
    # Exec
    "create_exec_response",
    "create_exec_request",
    # Web
    "create_browser_info",
    "create_web_scrape_result",
    "create_web_crawl_result",
    "create_web_crawl_response",
    "create_web_health_response",
    "create_browser_tab_info",
    "create_browser_action_response",
]
