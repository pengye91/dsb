"""Integration tests for the Config API."""

import os

import pytest

from dsb_sdk import DSBClient


@pytest.fixture
def client():
    """Create a DSB client for integration tests."""
    api_url = os.environ.get("DSB_API_URL", "http://localhost:18080")
    api_key = os.environ.get("DSB_API_KEY", "test-admin-key-for-testing-only")
    return DSBClient(api_url=api_url, api_key=api_key, timeout=120.0)


class TestConfigAPI:
    """Integration tests for the Config API."""

    def test_get_config(self, client):
        """Get config returns a dict with expected fields."""
        config = client.config.get()
        assert isinstance(config, dict)
        assert "default_sandbox_image" in config
        assert "authentication_required" in config

    def test_config_has_default_image(self, client):
        """Config should specify a default sandbox image."""
        config = client.config.get()
        assert isinstance(config["default_sandbox_image"], str)
        assert len(config["default_sandbox_image"]) > 0

    def test_config_authentication_required(self, client):
        """Test environment should have authentication enabled."""
        config = client.config.get()
        assert config["authentication_required"] is True

    def test_config_has_inactivity_timeout(self, client):
        """Config should specify an inactivity timeout."""
        config = client.config.get()
        assert "default_inactivity_timeout" in config
        assert isinstance(config["default_inactivity_timeout"], (int, float))
        assert config["default_inactivity_timeout"] > 0
