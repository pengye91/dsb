"""Utility functions for DSB SDK"""

from dsb_sdk.utils.circuit import (
    CircuitBreakerRegistry,
    CircuitBreakers,
    CircuitState,
    DSBCircuitBreaker,
    get_all_circuit_breaker_status,
    get_circuit_breaker,
    reset_all_circuit_breakers,
)
from dsb_sdk.utils.exec_error_handler import (
    parse_exec_result,
)
from dsb_sdk.utils.retry import (
    RetryConfig,
    RetryStrategies,
    retry_with_config,
    retry_with_exponential_backoff,
    should_retry_exception,
)
from dsb_sdk.utils.streaming import SSEDecoder
from dsb_sdk.utils.websocket import (
    AsyncWebSocketTerminalClient,
    WebSocketTerminalClient,
)

__all__ = [
    # Circuit breaker
    "CircuitState",
    "DSBCircuitBreaker",
    "CircuitBreakerRegistry",
    "CircuitBreakers",
    "get_circuit_breaker",
    "get_all_circuit_breaker_status",
    "reset_all_circuit_breakers",
    # Error handler
    "parse_exec_result",
    # Retry
    "RetryConfig",
    "RetryStrategies",
    "retry_with_config",
    "retry_with_exponential_backoff",
    "should_retry_exception",
    # Streaming & WebSocket
    "SSEDecoder",
    "WebSocketTerminalClient",
    "AsyncWebSocketTerminalClient",
]
