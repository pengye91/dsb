"""
Configuration loading for DSB SDK.

Supports loading configuration from:
- Environment variables (DSB_* prefix)
- .env file
- dsb.yaml / dsb.yml file
- CLI arguments (parsed separately)
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

import yaml


class DSBConfig:
    """Configuration for DSB client."""

    def __init__(
        self,
        api_url: str = "http://localhost:8080",
        timeout: float = 30.0,
        verify_ssl: bool = True,
        api_key: str | None = None,
    ):
        """
        Initialize configuration.

        Args:
            api_url: Base URL for DSB API
            timeout: Request timeout in seconds
            verify_ssl: Whether to verify SSL certificates
            api_key: Optional API key for authentication
        """
        self.api_url = api_url
        self.timeout = timeout
        self.verify_ssl = verify_ssl
        self.api_key = api_key

    @classmethod
    def load(cls, config_path: str | None = None) -> DSBConfig:
        """
        Load configuration from environment and optional config file.

        Priority order (highest to lowest):
        1. CLI arguments (passed directly)
        2. Environment variables (DSB_*)
        3. .env file
        4. dsb.yaml / dsb.yml file
        5. Default values

        Args:
            config_path: Optional path to config file (dsb.yaml or .env)
                     If None, searches in current directory and common locations.

        Returns:
            DSBConfig instance with loaded settings
        """
        # Start with defaults
        config = cls()

        # Try to load from file if provided or found
        if config_path:
            file_config = cls._load_from_file(config_path)
            config = cls._merge_configs(config, file_config)
        else:
            # Search for config files
            env_config = cls._load_from_env()
            yaml_config = cls._load_from_yaml()

            config = cls._merge_configs(config, yaml_config)
            config = cls._merge_configs(config, env_config)

        return config

    @classmethod
    def load_for_tests(cls) -> DSBConfig:
        """
        Load configuration for tests.

        Looks for:
        - .env.test file
        - dsb.test.yaml file
        - DSB_* environment variables with TEST_ prefix

        Returns:
            DSBConfig instance with test settings
        """
        config = cls()

        # Load from test-specific sources
        env_config = cls._load_from_env(prefix="TEST_")
        yaml_config = cls._load_from_yaml(suffix=".test")

        config = cls._merge_configs(config, yaml_config)
        config = cls._merge_configs(config, env_config)

        return config

    @staticmethod
    def _load_from_env(prefix: str = "DSB_") -> dict[str, Any]:
        """Load configuration from environment variables."""
        result: dict[str, Any] = {}

        # Map env var names to config fields
        env_mapping = {
            f"{prefix}API_URL": "api_url",
            f"{prefix}TIMEOUT": "timeout",
            f"{prefix}VERIFY_SSL": "verify_ssl",
            f"{prefix}API_KEY": "api_key",
        }

        for env_var, field_name in env_mapping.items():
            value = os.environ.get(env_var)
            if value is not None:
                if field_name == "timeout":
                    try:
                        value = float(value)
                    except ValueError:
                        continue
                elif field_name == "verify_ssl":
                    value = value.lower() in ("true", "1", "yes")
                result[field_name] = value

        return result

    @staticmethod
    def _load_from_yaml(suffix: str = "") -> dict[str, Any]:
        """Load configuration from dsb.yaml file."""
        result: dict[str, Any] = {}

        # Search paths
        search_paths = [
            Path.cwd() / f"dsb{suffix}.yaml",
            Path.cwd() / f"dsb{suffix}.yml",
            Path.home() / f".dsb{suffix}.yaml",
            Path.home() / f".dsb{suffix}.yml",
        ]

        for path in search_paths:
            if path.exists():
                try:
                    with open(path) as f:
                        data = yaml.safe_load(f) or {}
                    if "dsb" in data:
                        result = data["dsb"]
                    elif isinstance(data, dict):
                        result = data
                    break
                except Exception:
                    continue

        return result

    @classmethod
    def _load_from_file(cls, config_path: str) -> dict[str, Any]:
        """Load configuration from a specific file path."""
        path = Path(config_path)

        if not path.exists():
            return {}

        if path.suffix in (".yaml", ".yml"):
            return cls._load_from_yaml_file(path)
        elif path.suffix == ".env":
            return cls._load_from_dotenv_file(path)

        return {}

    @staticmethod
    def _load_from_yaml_file(path: Path) -> dict[str, Any]:
        """Load configuration from a YAML file."""
        try:
            with open(path) as f:
                data = yaml.safe_load(f) or {}
            if "dsb" in data:
                return data["dsb"]
            return data
        except Exception:
            return {}

    @staticmethod
    def _load_from_dotenv_file(path: Path) -> dict[str, Any]:
        """Load configuration from a .env file."""
        result: dict[str, Any] = {}

        try:
            with open(path) as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith("#") and "=" in line:
                        key, value = line.split("=", 1)
                        key = key.strip()
                        value = value.strip().strip('"').strip("'")

                        env_mapping = {
                            "DSB_API_URL": "api_url",
                            "DSB_TIMEOUT": "timeout",
                            "DSB_VERIFY_SSL": "verify_ssl",
                            "DSB_API_KEY": "api_key",
                        }

                        if key in env_mapping:
                            field_name = env_mapping[key]
                            if field_name == "timeout":
                                try:
                                    value = float(value)
                                except ValueError:
                                    continue
                            elif field_name == "verify_ssl":
                                value = value.lower() in ("true", "1", "yes")
                            result[field_name] = value
        except Exception:
            pass

        return result

    @classmethod
    def _merge_configs(cls, base: DSBConfig, override: dict[str, Any]) -> DSBConfig:
        """Merge override values into base configuration."""
        if not override:
            return base

        return cls(
            api_url=override.get("api_url", base.api_url),
            timeout=override.get("timeout", base.timeout),
            verify_ssl=override.get("verify_ssl", base.verify_ssl),
            api_key=override.get("api_key", base.api_key),
        )

    def to_dict(self) -> dict[str, Any]:
        """Convert configuration to dictionary."""
        return {
            "api_url": self.api_url,
            "timeout": self.timeout,
            "verify_ssl": self.verify_ssl,
            "api_key": self.api_key,
        }

    def __repr__(self) -> str:
        """String representation of configuration."""
        return (
            f"DSBConfig(api_url={self.api_url!r}, timeout={self.timeout}, "
            f"verify_ssl={self.verify_ssl}, api_key={'***' if self.api_key else None})"
        )
