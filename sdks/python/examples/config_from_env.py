"""
Example: Loading configuration from environment variables and files.

This example demonstrates how to use the DSBConfig class to load configuration
from environment variables, .env files, and YAML configuration files.
"""

from dsb_sdk import DSBClient, DSBConfig
from dsb_sdk.types.sandbox import PullPolicy, ResourceLimits


def example_environment_variables():
    """Example: Using environment variables for configuration."""
    # Set environment variables (in real usage, these would be set in your shell)
    import os

    os.environ["DSB_API_URL"] = "http://localhost:8080"
    os.environ["DSB_TIMEOUT"] = "60.0"
    os.environ["DSB_API_KEY"] = "your-api-key"

    # Load configuration from environment
    config = DSBConfig.load()

    print(f"API URL: {config.api_url}")
    print(f"Timeout: {config.timeout}")
    print(f"API Key: {'***' if config.api_key else None}")

    # Create client with loaded config
    client = DSBClient(
        api_url=config.api_url,
        timeout=config.timeout,
        api_key=config.api_key,
    )
    return client


def example_yaml_config():
    """Example: Using YAML configuration file."""
    # Create a YAML config file
    import tempfile

    import yaml

    config_data = {
        "dsb": {
            "api_url": "http://localhost:8080",
            "timeout": 30.0,
            "verify_ssl": False,
            "api_key": None,
        }
    }

    with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
        yaml.dump(config_data, f)
        config_path = f.name

    # Load configuration from YAML file
    config = DSBConfig.load(config_path=config_path)

    print(f"API URL: {config.api_url}")
    print(f"Timeout: {config.timeout}")
    print(f"Verify SSL: {config.verify_ssl}")

    # Clean up
    import os

    os.unlink(config_path)


def example_resource_limits():
    """Example: Creating a sandbox with resource limits."""
    # Define resource limits
    limits = ResourceLimits(
        memory_mb=512.0,  # 512 MB memory limit
        cpu_quota=50000,  # 50% CPU limit
        cpu_shares=512,  # CPU shares
        pids_limit=100,  # Max 100 processes
        ulimits={"nofile": {"soft": 1024, "hard": 4096}},  # File descriptor limits
    )

    # Create sandbox with resource limits
    client = DSBClient()

    sandbox = client.sandbox.create(
        image="python:3.12",
        name="resource-limited-sandbox",
        pull_policy=PullPolicy.MISSING,  # Only pull if not present
        resource_limits=limits,
        inactivity_timeout_minutes=30,  # Auto-stop after 30 min inactivity
    )

    print(f"Sandbox created: {sandbox.id}")
    print(f"State: {sandbox.state}")
    return sandbox


def example_pull_policy():
    """Example: Using pull policy to control image pulling."""
    client = DSBClient()

    # PullPolicy.ALWAYS - Always pull the image (default behavior)
    sandbox_always = client.sandbox.create(
        image="python:3.12",
        name="pull-always",
        pull_policy=PullPolicy.ALWAYS,
    )

    # PullPolicy.MISSING - Only pull if not present locally
    sandbox_missing = client.sandbox.create(
        image="python:3.12",
        name="pull-missing",
        pull_policy=PullPolicy.MISSING,
    )

    # PullPolicy.NEVER - Never pull, fail if image not present
    sandbox_never = client.sandbox.create(
        image="python:3.12",
        name="pull-never",
        pull_policy=PullPolicy.NEVER,
    )

    return sandbox_always, sandbox_missing, sandbox_never


def main():
    """Run all examples."""
    print("=" * 60)
    print("DSB SDK Configuration Examples")
    print("=" * 60)

    print("\n1. Environment Variables Example:")
    print("-" * 40)
    try:
        example_environment_variables()
    except Exception as e:
        print(f"Expected to fail without server: {e}")

    print("\n2. YAML Config File Example:")
    print("-" * 40)
    try:
        example_yaml_config()
    except Exception as e:
        print(f"Expected to fail without config file: {e}")

    print("\n3. Resource Limits Example:")
    print("-" * 40)
    try:
        example_resource_limits()
    except Exception as e:
        print(f"Expected to fail without server: {e}")

    print("\n4. Pull Policy Example:")
    print("-" * 40)
    try:
        example_pull_policy()
    except Exception as e:
        print(f"Expected to fail without server: {e}")

    print("\n" + "=" * 60)
    print("Examples completed!")
    print("=" * 60)


if __name__ == "__main__":
    main()
