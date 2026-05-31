"""
Circuit breaker implementation for preventing cascading failures.

Circuit breakers stop calling services that are failing repeatedly,
allowing them to recover and preventing system-wide outages.
"""

import logging
from collections.abc import Callable
from enum import Enum
from functools import wraps
from typing import Any

from pybreaker import CircuitBreaker, CircuitBreakerError

from dsb_sdk.exceptions import DSBConnectionError, DSBTimeoutError

logger = logging.getLogger(__name__)


class CircuitState(Enum):
    """Circuit breaker states."""

    CLOSED = "closed"  # Normal operation, requests allowed
    OPEN = "open"  # Circuit is open, requests blocked
    HALF_OPEN = "half_open"  # Testing if service has recovered


# Default circuit breaker settings
DEFAULT_FAIL_MAX = 5  # Open circuit after 5 failures
DEFAULT_TIMEOUT_DURATION = 60  # Reset after 60 seconds
DEFAULT_EXCEPTIONS = (DSBConnectionError, DSBTimeoutError, Exception)


class DSBCircuitBreaker:
    """
    Circuit breaker for DSB API calls.

    Prevents cascading failures by stopping calls to a failing service
    after a threshold of failures is reached.

    States:
    - CLOSED: Normal operation, requests are allowed
    - OPEN: Too many failures, requests are blocked
    - HALF_OPEN: Testing if service has recovered

    Example:
        ```python
        # Create a circuit breaker
        breaker = DSBCircuitBreaker(
            fail_max=5,
            timeout_duration=60,
            name="sandbox-api"
        )

        # Use as decorator
        @breaker
        def create_sandbox(image: str):
            return client.sandbox.create(image=image)

        # Use as context manager
        with breaker:
            result = client.sandbox.create(image="python:3.12")
        ```
    """

    def __init__(
        self,
        fail_max: int = DEFAULT_FAIL_MAX,
        timeout_duration: int = DEFAULT_TIMEOUT_DURATION,
        exceptions: tuple = DEFAULT_EXCEPTIONS,
        name: str = "dsb-circuit-breaker",
    ):
        """
        Initialize circuit breaker.

        Args:
            fail_max: Number of failures before opening circuit (default: 5)
            timeout_duration: Seconds to wait before trying again (default: 60)
            exceptions: Exception types that count as failures
            name: Name for this circuit breaker (for logging)
        """
        self._breaker = CircuitBreaker(
            fail_max=fail_max,
            reset_timeout=timeout_duration,
        )
        # Note: pybreaker counts all exceptions as failures by default.
        # The exceptions parameter here is for documentation purposes only.
        # Use add_excluded_exception() to exclude specific exceptions from failure counting.
        self.name = name
        self.fail_max = fail_max
        self.timeout_duration = timeout_duration

    def __call__(self, func: Callable) -> Callable:
        """
        Decorator for protecting function calls with circuit breaker.

        Args:
            func: Function to protect

        Returns:
            Wrapped function
        """

        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            try:
                return self._breaker.call(func, *args, **kwargs)
            except CircuitBreakerError as e:
                logger.warning(
                    f"Circuit breaker '{self.name}' is OPEN: {e}. "
                    f"Rejecting request to {func.__name__}"
                )
                raise DSBConnectionError(f"Service unavailable (circuit breaker open): {e}") from e

        return wrapper

    def __enter__(self):
        """Context manager entry."""
        self._breaker.__enter__()  # type: ignore
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        return self._breaker.__exit__(exc_type, exc_val, exc_tb)  # type: ignore

    @property
    def state(self) -> CircuitState:
        """Get current circuit breaker state."""
        pybreaker_state = self._breaker.current_state

        if pybreaker_state == "closed":
            return CircuitState.CLOSED
        elif pybreaker_state == "open":
            return CircuitState.OPEN
        elif pybreaker_state == "half-open":
            return CircuitState.HALF_OPEN
        else:
            raise ValueError(f"Unknown circuit breaker state: {pybreaker_state}")

    @property
    def failure_count(self) -> int:
        """Get current failure count."""
        return self._breaker.fail_counter  # type: ignore

    @property
    def is_open(self) -> bool:
        """Check if circuit is open (requests blocked)."""
        return self.state == CircuitState.OPEN

    def reset(self):
        """Manually reset the circuit breaker to CLOSED state."""
        self._breaker = CircuitBreaker(
            fail_max=self.fail_max,
            reset_timeout=self.timeout_duration,
        )
        logger.info(f"Circuit breaker '{self.name}' manually reset to CLOSED")

    def force_open(self):
        """Manually open the circuit breaker (block requests)."""
        self._breaker.open()
        logger.warning(f"Circuit breaker '{self.name}' manually forced OPEN")


class CircuitBreakerRegistry:
    """
    Registry for managing multiple circuit breakers.

    Example:
        ```python
        # Get or create circuit breakers by name
        registry = CircuitBreakerRegistry()

        sandbox_breaker = registry.get("sandbox-api", fail_max=5)
        ssh_breaker = registry.get("ssh-api", fail_max=3)

        # Check status of all circuit breakers
        registry.print_status()
        ```
    """

    def __init__(self):
        self._breakers: dict[str, DSBCircuitBreaker] = {}

    def get(
        self,
        name: str,
        fail_max: int = DEFAULT_FAIL_MAX,
        timeout_duration: int = DEFAULT_TIMEOUT_DURATION,
    ) -> DSBCircuitBreaker:
        """
        Get or create a circuit breaker by name.

        Args:
            name: Circuit breaker name
            fail_max: Failures before opening (only used on creation)
            timeout_duration: Reset timeout (only used on creation)

        Returns:
            DSBCircuitBreaker instance
        """
        if name not in self._breakers:
            self._breakers[name] = DSBCircuitBreaker(
                fail_max=fail_max,
                timeout_duration=timeout_duration,
                name=name,
            )
            logger.info(f"Created new circuit breaker: {name}")

        return self._breakers[name]

    def reset_all(self):
        """Reset all circuit breakers to CLOSED state."""
        for name, breaker in self._breakers.items():
            breaker.reset()
        logger.info("All circuit breakers reset")

    def open_all(self):
        """Force all circuit breakers OPEN (block all requests)."""
        for name, breaker in self._breakers.items():
            breaker.force_open()
        logger.warning("All circuit breakers forced OPEN")

    def get_status(self) -> dict[str, dict[str, Any]]:
        """
        Get status of all circuit breakers.

        Returns:
            Dict mapping breaker names to their status
        """
        return {
            name: {
                "state": breaker.state.value,
                "failure_count": breaker.failure_count,
                "is_open": breaker.is_open,
            }
            for name, breaker in self._breakers.items()
        }

    def print_status(self):
        """Print status of all circuit breakers to log."""
        status = self.get_status()
        logger.info("Circuit Breaker Status:")
        for name, info in status.items():
            logger.info(
                f"  {name}: {info['state']} "
                f"(failures: {info['failure_count']}, "
                f"open: {info['is_open']})"
            )


# Global circuit breaker registry
_global_registry = CircuitBreakerRegistry()


def get_circuit_breaker(
    name: str,
    fail_max: int = DEFAULT_FAIL_MAX,
    timeout_duration: int = DEFAULT_TIMEOUT_DURATION,
) -> DSBCircuitBreaker:
    """
    Get or create a circuit breaker from the global registry.

    Args:
        name: Circuit breaker name
        fail_max: Failures before opening
        timeout_duration: Reset timeout in seconds

    Returns:
        DSBCircuitBreaker instance

    Example:
        ```python
        from dsb_sdk.utils.circuit import get_circuit_breaker

        # Get a shared circuit breaker
        breaker = get_circuit_breaker("sandbox-api", fail_max=5)

        @breaker
        def create_sandbox(image: str):
            return client.sandbox.create(image=image)
        ```
    """
    return _global_registry.get(
        name=name,
        fail_max=fail_max,
        timeout_duration=timeout_duration,
    )


def reset_all_circuit_breakers():
    """Reset all circuit breakers in the global registry."""
    _global_registry.reset_all()


def get_all_circuit_breaker_status() -> dict[str, dict[str, Any]]:
    """Get status of all circuit breakers in the global registry."""
    return _global_registry.get_status()


# Pre-configured circuit breakers for DSB services
class CircuitBreakers:
    """
    Pre-configured circuit breakers for common DSB services.

    Example:
        ```python
        from dsb_sdk.utils.circuit import CircuitBreakers

        @CircuitBreakers.sandbox
        def create_sandbox(image: str):
            return client.sandbox.create(image=image)

        @CircuitBreakers.ssh
        def create_ssh_session(sandbox_id: str):
            return client.ssh.create(sandbox_id=sandbox_id)
        ```
    """

    # Sandbox API - relatively stable
    sandbox = get_circuit_breaker("sandbox-api", fail_max=5, timeout_duration=60)

    # SSH API - can be flaky
    ssh = get_circuit_breaker("ssh-api", fail_max=3, timeout_duration=30)

    # Web scraping - external dependencies
    web = get_circuit_breaker("web-api", fail_max=10, timeout_duration=120)

    # Terminal API - stateful, can fail
    terminal = get_circuit_breaker("terminal-api", fail_max=3, timeout_duration=45)

    # Health checks - should always work
    health = get_circuit_breaker("health-api", fail_max=2, timeout_duration=30)
