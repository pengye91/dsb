"""
Example: Port forwarding and exposing sandbox ports.

This example demonstrates how to:
- Expose ports from sandboxes
- Access services running inside sandboxes
- Configure port mappings
"""

from dsb_sdk import DSBClient


def example_basic_port_mapping():
    """Example: Basic port mapping."""
    print("=" * 60)
    print("Basic Port Mapping Example")
    print("=" * 60)

    client = DSBClient()

    # Define port mappings (host_port: container_port)
    ports = {
        "8080": "80",  # Map host 8080 to container 80
        "8443": "443",  # Map host 8443 to container 443
    }

    print("\n1. Creating sandbox with port mappings...")
    try:
        sandbox = client.sandbox.create(
            image="python:3.12",
            name="port-test",
            ports=ports,
        )
        print(f"   Sandbox created: {sandbox.id}")
        print(f"   Ports mapped: {ports}")

        # Show how to access the service
        print("\n2. Accessing service:")
        print("   The service should be accessible at:")
        print("   - http://localhost:8080")
        print("   - https://localhost:8443")

        # Verify ports are configured
        print("\n3. Checking sandbox configuration...")
        sandbox = client.sandbox.get(sandbox.id)
        print(f"   Config ports: {sandbox.config.ports}")

        client.sandbox.delete(sandbox.id)
        print("\n4. Sandbox deleted")

    except Exception as e:
        print(f"   Note: {e}")
        print("   (Expected without running DSB server)")


def example_dynamic_port():
    """Example: Using dynamic port assignment."""
    print("\n" + "=" * 60)
    print("Dynamic Port Assignment Example")
    print("=" * 60)

    client = DSBClient()

    print("\n1. Creating sandbox with auto-assigned ports...")
    try:
        # Let the server assign ports automatically
        sandbox = client.sandbox.create(
            image="python:3.12",
            name="dynamic-port-test",
            ports={
                "0": "8000",  # 0 means auto-assign host port
            },
        )
        print(f"   Sandbox created: {sandbox.id}")

        # Get the actual assigned port
        sandbox = client.sandbox.get(sandbox.id)
        assigned_port = list(sandbox.config.ports.keys())[0]
        container_port = sandbox.config.ports[assigned_port]

        print(f"   Container port: {container_port}")
        print(f"   Assigned host port: {assigned_port}")
        print(f"   Access service at: localhost:{assigned_port}")

        client.sandbox.delete(sandbox.id)

    except Exception as e:
        print(f"   Note: {e}")


def example_multi_port_service():
    """Example: Running a multi-port service."""
    print("\n" + "=" * 60)
    print("Multi-Port Service Example")
    print("=" * 60)

    client = DSBClient()

    # Run a service that exposes multiple ports
    ports = {
        "3000": "3000",  # HTTP API
        "3001": "3001",  # Metrics endpoint
        "9090": "9090",  # Admin interface
    }

    print("\n1. Creating sandbox for multi-port service...")
    try:
        sandbox = client.sandbox.create(
            image="python:3.12",
            name="multi-port-test",
            ports=ports,
            command=["python", "-m", "http.server", "3000"],
            environment={
                "METRICS_PORT": "3001",
                "ADMIN_PORT": "9090",
            },
        )
        print(f"   Sandbox created: {sandbox.id}")
        print(f"   Ports: {ports}")

        # Show all access points
        print("\n2. Service endpoints:")
        for host_port, container_port in ports.items():
            print(f"   - localhost:{host_port} -> container:{container_port}")

        client.sandbox.delete(sandbox.id)

    except Exception as e:
        print(f"   Note: {e}")


def example_tcp_udp_ports():
    """Example: TCP and UDP port configuration."""
    print("\n" + "=" * 60)
    print("TCP/UDP Port Example")
    print("=" * 60)

    client = DSBClient()

    print("\n1. Creating sandbox with various port types...")
    try:
        # Most services use TCP
        sandbox = client.sandbox.create(
            image="python:3.12",
            name="tcp-udp-test",
            ports={
                "80": "80",  # HTTP (TCP)
                "53": "53/udp",  # DNS (UDP)
                "5000": "5000",  # Custom protocol (TCP)
            },
        )
        print(f"   Sandbox created: {sandbox.id}")
        print("   Ports configured:")
        print("   - 80/tcp - HTTP")
        print("   - 53/udp - DNS")
        print("   - 5000/tcp - Custom")

        client.sandbox.delete(sandbox.id)

    except Exception as e:
        print(f"   Note: {e}")


def example_port_security():
    """Example: Security considerations for exposed ports."""
    print("\n" + "=" * 60)
    print("Port Security Example")
    print("=" * 60)

    print("\n1. Security best practices:")
    print("   - Only expose ports that are necessary")
    print("   - Use firewall rules to restrict access")
    print("   - Consider using authentication on exposed services")
    print("   - Monitor port access patterns")

    print("\n2. Example: Secure port configuration")
    print("""
    # Don't expose unnecessary ports
    ports = {
        "8080": "80",  # Only expose what's needed
    }

    # Use environment variables for sensitive config
    environment = {
        "API_KEY": "your-key-here",  # Not in code
        "DATABASE_URL": "postgresql://...",
    }

    # Consider using a reverse proxy for public services
    """)


def main():
    """Run all examples."""
    print("\n" + "#" * 60)
    print("# DSB SDK Port Forwarding Examples")
    print("#" * 60)

    examples = [
        ("Basic Port Mapping", example_basic_port_mapping),
        ("Dynamic Port Assignment", example_dynamic_port),
        ("Multi-Port Service", example_multi_port_service),
        ("TCP/UDP Ports", example_tcp_udp_ports),
        ("Port Security", example_port_security),
    ]

    for name, func in examples:
        try:
            func()
        except Exception as e:
            print(f"\n   Error in {name}: {e}")

    print("\n" + "#" * 60)
    print("# Examples completed!")
    print("#" * 60)


if __name__ == "__main__":
    main()
