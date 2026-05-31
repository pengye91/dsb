"""
Example: Volume mounts and file operations with sandboxes.

This example demonstrates how to:
- Mount host directories as volumes
- Upload files to sandboxes
- Download files from sandboxes
"""

import os
import tempfile

from dsb_sdk import DSBClient


def example_volume_mounts():
    """Example: Mounting host directories as volumes."""
    print("=" * 60)
    print("Volume Mounts Example")
    print("=" * 60)

    client = DSBClient()

    # Create a temporary directory for mounting
    with tempfile.TemporaryDirectory() as temp_dir:
        # Create some files in the host directory
        host_file = os.path.join(temp_dir, "test.txt")
        with open(host_file, "w") as f:
            f.write("Hello from host!\n")

        # Create a subdirectory
        subdir = os.path.join(temp_dir, "data")
        os.makedirs(subdir)
        with open(os.path.join(subdir, "data.txt"), "w") as f:
            f.write("Data file content\n")

        # Define volume mounts
        volumes = {
            temp_dir: "/mnt/host",  # Mount entire temp dir
        }

        print("\n1. Creating sandbox with volume mount...")
        try:
            sandbox = client.sandbox.create(
                image="python:3.12",
                name="volume-test",
                volumes=volumes,
            )
            print(f"   Sandbox created: {sandbox.id}")
            print(f"   Mounted {temp_dir} -> /mnt/host")

            # Verify the mount
            print("\n2. Verifying volume mount...")
            result = client.sandbox.exec(
                sandbox.id,
                ["cat", "/mnt/host/test.txt"],
            )
            print(f"   File content: {result.get('output', '').strip()}")

            # List mounted directory
            result = client.sandbox.exec(
                sandbox.id,
                ["ls", "-la", "/mnt/host/"],
            )
            print(f"   Directory listing:\n   {result.get('output', '')}")

            # Cleanup
            client.sandbox.delete(sandbox.id)
            print("\n3. Sandbox deleted")

        except Exception as e:
            print(f"   Note: {e}")
            print("   (Expected without running DSB server)")


def example_readonly_volume():
    """Example: Mounting read-only volumes."""
    print("\n" + "=" * 60)
    print("Read-Only Volume Example")
    print("=" * 60)

    client = DSBClient()

    with tempfile.TemporaryDirectory() as temp_dir:
        # Create a file to share
        config_file = os.path.join(temp_dir, "config.json")
        with open(config_file, "w") as f:
            f.write('{"key": "value", "setting": true}\n')

        # Mount as read-only (use :ro suffix)
        volumes = {
            temp_dir: "/mnt/config:ro",  # Read-only mount
        }

        print("\n1. Creating sandbox with read-only volume...")
        try:
            sandbox = client.sandbox.create(
                image="python:3.12",
                name="readonly-volume-test",
                volumes=volumes,
            )
            print(f"   Sandbox created: {sandbox.id}")

            # Try to write to read-only mount (should fail)
            print("\n2. Testing read-only enforcement...")
            result = client.sandbox.exec(
                sandbox.id,
                ["sh", "-c", "echo 'test' > /mnt/config/new.txt 2>&1 || true"],
            )
            output = result.get("output", "")
            if "Read-only file system" in output or "Permission denied" in output:
                print("   Read-only enforcement working correctly")
            else:
                print(f"   Output: {output}")

            # Read the config file
            result = client.sandbox.exec(
                sandbox.id,
                ["cat", "/mnt/config/config.json"],
            )
            print(f"   Config content: {result.get('output', '').strip()}")

            client.sandbox.delete(sandbox.id)

        except Exception as e:
            print(f"   Note: {e}")


def example_upload_file():
    """Example: Uploading files to sandbox."""
    print("\n" + "=" * 60)
    print("File Upload Example")
    print("=" * 60)

    client = DSBClient()

    with tempfile.TemporaryDirectory() as temp_dir:
        # Create a script to upload
        script_path = os.path.join(temp_dir, "upload_script.py")
        with open(script_path, "w") as f:
            f.write("""
import sys
print(f"Hello from {sys.argv[1]}!")

# Write output to a file
with open("/tmp/output.txt", "w") as f:
    f.write("Script executed successfully\\n")
""")

        print("\n1. Creating sandbox...")
        try:
            sandbox = client.sandbox.create(
                image="python:3.12",
                name="upload-test",
            )
            print(f"   Sandbox created: {sandbox.id}")

            # Upload file using base64 encoding
            print("\n2. Uploading file to sandbox...")
            with open(script_path, "rb") as f:
                content = f.read()
            import base64

            encoded = base64.b64encode(content).decode()

            # Write file in sandbox
            client.sandbox.exec(
                sandbox.id,
                ["sh", "-c", f"echo '{encoded}' | base64 -d > /tmp/upload_script.py"],
            )
            print("   File uploaded")

            # Execute the uploaded script
            print("\n3. Executing uploaded script...")
            result = client.sandbox.exec(
                sandbox.id,
                ["python", "/tmp/upload_script.py", "uploaded"],
            )
            print(f"   Output: {result.get('output', '').strip()}")

            # Verify output file
            print("\n4. Verifying output file...")
            result = client.sandbox.exec(
                sandbox.id,
                ["cat", "/tmp/output.txt"],
            )
            print(f"   Content: {result.get('output', '').strip()}")

            client.sandbox.delete(sandbox.id)

        except Exception as e:
            print(f"   Note: {e}")


def example_download_file():
    """Example: Downloading files from sandbox."""
    print("\n" + "=" * 60)
    print("File Download Example")
    print("=" * 60)

    client = DSBClient()

    print("\n1. Creating sandbox...")
    try:
        sandbox = client.sandbox.create(
            image="python:3.12",
            name="download-test",
        )
        print(f"   Sandbox created: {sandbox.id}")

        # Create a file in the sandbox
        print("\n2. Creating file in sandbox...")
        client.sandbox.exec(
            sandbox.id,
            ["sh", "-c", "echo 'Download test content' > /tmp/download_me.txt"],
        )
        client.sandbox.exec(
            sandbox.id,
            ["sh", "-c", "echo 'Another file' > /tmp/another.txt"],
        )
        print("   Files created")

        # Download file using base64 encoding
        print("\n3. Downloading file from sandbox...")
        result = client.sandbox.exec(
            sandbox.id,
            ["base64", "/tmp/download_me.txt"],
        )
        encoded = result.get("output", "").strip()
        import base64

        content = base64.b64decode(encoded).decode()
        print(f"   Downloaded content: {content.strip()}")

        # Download multiple files
        print("\n4. Downloading multiple files...")
        with tempfile.TemporaryDirectory() as temp_dir:
            for filename in ["download_me.txt", "another.txt"]:
                result = client.sandbox.exec(
                    sandbox.id,
                    ["base64", f"/tmp/{filename}"],
                )
                encoded = result.get("output", "").strip()
                content = base64.b64decode(encoded).decode()
                local_path = os.path.join(temp_dir, filename)
                with open(local_path, "w") as f:
                    f.write(content)
                print(f"   Saved {filename} to {local_path}")

        client.sandbox.delete(sandbox.id)

    except Exception as e:
        print(f"   Note: {e}")


def main():
    """Run all examples."""
    print("\n" + "#" * 60)
    print("# DSB SDK Volume & File Operations Examples")
    print("#" * 60)

    examples = [
        ("Volume Mounts", example_volume_mounts),
        ("Read-Only Volume", example_readonly_volume),
        ("File Upload", example_upload_file),
        ("File Download", example_download_file),
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
