"""
DSB SDK Constants

Shared constants used throughout the Python SDK.
"""

# Default timeouts in seconds (mirrors server-side defaults)
DEFAULT_WEB_TOOLS_TIMEOUT = 90  # Increased to 90 seconds for crawl operations
DEFAULT_BROWSER_TOOLS_TIMEOUT = 120
DEFAULT_DATABEND_TIMEOUT = 60

# HTTP client buffer time added to tool timeouts to account for network overhead
# HTTP timeout = tool_timeout + HTTP_BUFFER_SECS
DEFAULT_HTTP_BUFFER_SECS = 30

# Maximum allowed timeout for custom operations (5 minutes)
MAX_ALLOWED_TIMEOUT = 300

# Maximum browser tabs per sandbox (default: 20)
# Matches server-side sandbox.max_browser_tabs config
DEFAULT_MAX_BROWSER_TABS = 20
