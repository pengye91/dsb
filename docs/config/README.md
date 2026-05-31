# Configuration Guide

DSB uses a robust, environment-variable-based configuration system to manage infrastructure and application parameters.

## Infrastructure Configuration (`.env`)

The infrastructure is configured using `.env` files, which are managed via `Makefile` targets.

### Managing Environments

To switch between configurations, use the provided Makefile targets:

- **`make config-default`**: Configures the environment to use standard international public mirrors (`docker.io`, `pypi.org`, etc.).
- **`make config-china`**: Configures the environment to use China-accessible mirrors (e.g., `Tsinghua`, `Aliyun`) for faster builds and dependencies.

This command generates or updates the `.env` file used by `docker-compose`.

## Application Configuration (`dsb.yaml`)

The DSB server configuration is defined in `dsb.yaml`.

- **Templates**: Use `dsb.yaml.example` as a starting point.
- **Loading**: The system loads configuration based on the environment:
  - Production: `dsb::config::load()`
  - Tests: `dsb::config::load_for_tests()`

**Rule**: Never hardcode configuration values. Use the provided loading functions.

## Adding New Configuration Parameters

If you need to add new configuration parameters:

1. Update `src/config/types.rs` to include the new parameter.
2. Update `src/config/mod.rs` to ensure it is loaded correctly.
3. Update `dsb.yaml.example` to document the new parameter.
4. Ensure the parameter is accessible through the `dsb::config::load()` mechanism.
