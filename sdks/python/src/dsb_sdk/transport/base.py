"""
Base transport types for DSB SDK.

This module provides re-exports for backward compatibility.
New code should use SyncTransport and AsyncTransport directly.
"""

# Re-export for backward compatibility
from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.transport.sync import SyncTransport

__all__ = ["SyncTransport", "AsyncTransport"]
