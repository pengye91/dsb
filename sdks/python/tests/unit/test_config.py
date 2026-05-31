"""Unit tests for DSBConfig configuration loading."""

import os
import tempfile
from pathlib import Path

from dsb_sdk.config import DSBConfig


class TestDSBConfig:
    """Tests for DSBConfig class."""

    def test_default_values(self):
        """Test that default values are applied."""
        config = DSBConfig()
        assert config.api_url == "http://localhost:8080"
        assert config.timeout == 30.0
        assert config.verify_ssl is True
        assert config.api_key is None

    def test_custom_values(self):
        """Test that custom values are applied."""
        config = DSBConfig(
            api_url="https://example.com",
            timeout=60.0,
            verify_ssl=False,
            api_key="test-key",
        )
        assert config.api_url == "https://example.com"
        assert config.timeout == 60.0
        assert config.verify_ssl is False
        assert config.api_key == "test-key"

    def test_to_dict(self):
        """Test conversion to dictionary."""
        config = DSBConfig(
            api_url="https://example.com",
            timeout=45.0,
            api_key="secret",
        )
        result = config.to_dict()
        assert result["api_url"] == "https://example.com"
        assert result["timeout"] == 45.0
        assert result["verify_ssl"] is True
        assert result["api_key"] == "secret"

    def test_repr(self):
        """Test string representation."""
        config = DSBConfig(api_key="secret")
        repr_str = repr(config)
        assert "DSBConfig" in repr_str
        assert "secret" not in repr_str  # Should be masked

    def test_load_from_env(self):
        """Test loading from environment variables."""
        # Set environment variables
        os.environ["DSB_API_URL"] = "https://env-example.com"
        os.environ["DSB_TIMEOUT"] = "45.0"
        os.environ["DSB_API_KEY"] = "env-key"

        try:
            config = DSBConfig.load()
            assert config.api_url == "https://env-example.com"
            assert config.timeout == 45.0
            assert config.api_key == "env-key"
        finally:
            # Cleanup
            del os.environ["DSB_API_URL"]
            del os.environ["DSB_TIMEOUT"]
            del os.environ["DSB_API_KEY"]

    def test_load_from_env_with_prefix(self):
        """Test loading from environment variables with prefix."""
        # Set test environment variables
        os.environ["TEST_DSB_API_URL"] = "https://test-env.com"
        os.environ["TEST_DSB_TIMEOUT"] = "90.0"

        try:
            config = DSBConfig._load_from_env(prefix="TEST_DSB_")
            assert config["api_url"] == "https://test-env.com"
            assert config["timeout"] == 90.0
        finally:
            del os.environ["TEST_DSB_API_URL"]
            del os.environ["TEST_DSB_TIMEOUT"]

    def test_load_from_yaml(self):
        """Test loading from YAML file."""
        config_data = {
            "dsb": {
                "api_url": "https://yaml-example.com",
                "timeout": 120.0,
                "verify_ssl": False,
            }
        }

        with tempfile.TemporaryDirectory() as tmpdir:
            yaml_path = Path(tmpdir) / "dsb.yaml"
            import yaml

            with open(yaml_path, "w") as f:
                yaml.dump(config_data, f)

            config = DSBConfig._load_from_yaml_file(yaml_path)
            assert config["api_url"] == "https://yaml-example.com"
            assert config["timeout"] == 120.0
            assert config["verify_ssl"] is False

    def test_load_from_dotenv(self):
        """Test loading from .env file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            env_path = Path(tmpdir) / ".env"
            with open(env_path, "w") as f:
                f.write("DSB_API_URL=https://dotenv-example.com\n")
                f.write("DSB_TIMEOUT=75.0\n")
                f.write("DSB_API_KEY=dotenv-key\n")

            config = DSBConfig._load_from_dotenv_file(env_path)
            assert config["api_url"] == "https://dotenv-example.com"
            assert config["timeout"] == 75.0
            assert config["api_key"] == "dotenv-key"

    def test_load_from_file_not_found(self):
        """Test loading from non-existent file returns empty dict."""
        config = DSBConfig._load_from_file("/non/existent/path.yaml")
        assert config == {}

    def test_load_merges_configs(self):
        """Test that load merges configs correctly."""
        # Test merge with defaults
        config = DSBConfig._merge_configs(
            DSBConfig(),
            {"api_url": "https://override.com", "timeout": 200.0},
        )
        assert config.api_url == "https://override.com"
        assert config.timeout == 200.0
        # Unspecified values should remain from defaults
        assert config.verify_ssl is True

    def test_load_for_tests(self):
        """Test loading test configuration."""
        # Set test environment variables
        os.environ["TEST_DSB_API_URL"] = "https://test-server.com"
        os.environ["TEST_DSB_TIMEOUT"] = "30.0"

        try:
            config = DSBConfig.load_for_tests()
            # Note: load_for_tests uses YAML search but env vars override
            # Since we don't create test YAML file, env vars should be used
            assert config.timeout == 30.0
        finally:
            del os.environ["TEST_DSB_API_URL"]
            del os.environ["TEST_DSB_TIMEOUT"]

    def test_load_with_file_path(self):
        """Test loading with explicit file path."""
        with tempfile.TemporaryDirectory() as tmpdir:
            yaml_path = Path(tmpdir) / "custom-config.yaml"
            import yaml

            with open(yaml_path, "w") as f:
                yaml.dump({"dsb": {"api_url": "https://custom.com"}}, f)

            config = DSBConfig.load(config_path=str(yaml_path))
            assert config.api_url == "https://custom.com"

    def test_env_timeout_conversion(self):
        """Test that timeout environment variable is converted to float."""
        os.environ["DSB_TIMEOUT"] = "not-a-number"

        try:
            config = DSBConfig._load_from_env()
            # Should not include timeout if conversion fails
            assert "timeout" not in config
        finally:
            del os.environ["DSB_TIMEOUT"]

    def test_env_verify_ssl_conversion(self):
        """Test that verify_ssl environment variable is converted to bool."""
        os.environ["DSB_VERIFY_SSL"] = "false"

        try:
            config = DSBConfig._load_from_env()
            assert config["verify_ssl"] is False

            os.environ["DSB_VERIFY_SSL"] = "true"
            config = DSBConfig._load_from_env()
            assert config["verify_ssl"] is True

            os.environ["DSB_VERIFY_SSL"] = "1"
            config = DSBConfig._load_from_env()
            assert config["verify_ssl"] is True
        finally:
            del os.environ["DSB_VERIFY_SSL"]


class TestDSBConfigLoad:
    """Tests for DSBConfig.load() method."""

    def test_load_defaults(self):
        """Test load with no config sources."""
        # Temporarily remove DSB_API_URL env var if set to test true defaults
        original_url = os.environ.get("DSB_API_URL")
        if "DSB_API_URL" in os.environ:
            del os.environ["DSB_API_URL"]

        try:
            config = DSBConfig.load()
            # Should use defaults when no config sources exist
            assert config.api_url == "http://localhost:8080"
        finally:
            # Restore original value
            if original_url:
                os.environ["DSB_API_URL"] = original_url

    def test_load_env_overrides_yaml(self):
        """Test that environment variables override YAML."""
        with tempfile.TemporaryDirectory() as tmpdir:
            yaml_path = Path(tmpdir) / "dsb.yaml"
            import yaml

            with open(yaml_path, "w") as f:
                yaml.dump({"dsb": {"api_url": "https://yaml.com"}}, f)

            os.environ["DSB_API_URL"] = "https://env-override.com"

            try:
                config = DSBConfig.load()
                # Environment should take priority over YAML (our implementation)
                assert config.api_url == "https://env-override.com"
            finally:
                del os.environ["DSB_API_URL"]

    def test_load_yaml_only(self):
        """Test that YAML config is applied when no env vars set."""
        with tempfile.TemporaryDirectory() as tmpdir:
            yaml_path = Path(tmpdir) / "dsb.yaml"
            import yaml

            with open(yaml_path, "w") as f:
                yaml.dump({"dsb": {"api_url": "https://yaml-only.com"}}, f)

            config = DSBConfig.load(config_path=str(yaml_path))
            assert config.api_url == "https://yaml-only.com"
