# Contributing to DSB

Thank you for considering contributing to DSB (Distributed Sandboxes). We welcome contributions from everyone and are grateful for your time and effort.

> **First time contributing?** Look for issues labeled [`good first issue`](https://github.com/pengye91/dsb/issues?q=is%3Aopen+is%3Aissue+label%3A%22good+first+issue%22) — these are scoped for newcomers.

## Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How to Contribute

### Reporting Bugs

If you find a bug, please open a GitHub issue using the **Bug Report** template. Include as much detail as possible:

- A clear description of the bug
- Steps to reproduce the issue
- Expected vs. actual behavior
- Your environment (OS, Rust version, Docker version, backend — Docker or Kubernetes)
- Any relevant logs or error messages

### Suggesting Features

We welcome feature suggestions. Please open a GitHub issue using the **Feature Request** template and describe:

- The problem you are trying to solve
- Your proposed solution
- Any alternatives you have considered

### Improving Documentation

Documentation PRs are very welcome — typo fixes, clarification, additional examples, new guides. Open them the same way as code PRs.

### Code Contributions

1. **Fork** the repository
2. **Branch**: Create a feature branch from `main` with a descriptive name (e.g. `fix/sandbox-oom-recovery`, `feat/mcp-tool-rate-limiting`)
3. **Make your changes** — keep commits focused; see the commit message style below
4. **Verify locally**: `make test`, `cargo clippy --all-targets --all-features`, and `make audit`
5. **Push** to your fork and open a Pull Request against `main`
6. **CI** will run automatically; a maintainer will review and may request changes

> The `main` branch is the default and the only long-lived branch. All work lands directly on `main` via PRs.

## Development Setup

### Prerequisites

- Rust 1.88 or later
- Docker
- (Optional) Kubernetes cluster or `kind` for K8s backend testing
- (Optional) `uv` for the Python SDK

### Building

```bash
cargo build --release
```

### Testing

```bash
make test          # Full suite via Docker
make test-quick    # Unit tests only (no Docker)
```

### Linting

Run Clippy on all targets and features:

```bash
cargo clippy --all-targets --all-features
```

### Security Audit

```bash
make audit
```

## Code Standards

We maintain high code quality standards. Please follow these guidelines:

### Testing

- **Use inline test modules**: `#[cfg(test)] mod tests {`
- **Never use separate test files** (no `tests/something.rs` for unit tests; the `tests/` directory at the crate root is reserved for true integration tests)
- Test modules must be declared as `#[cfg(test)] mod name;`, **never** `pub mod name;`
- If a test fails, fix the underlying issue rather than ignoring it
- Use `tests/common/` fixtures (ResourceRegistry, TestDatabase) for shared setup

### Configuration

- **Never hardcode configuration values**
- Use `dsb::config::load()` in production code
- Use `dsb::config::load_for_tests()` in test code
- All configurable values must go through the `dsb.yaml` / `.env` system, not literals

### Documentation

- Add documentation to all public functions
- Add markdown module documentation
- Maintain the README.md
- Update the relevant doc in `docs/architecture/` when changing module internals

### Logging

- Use `tracing` logs with correct levels (`error!`, `warn!`, `info!`, `debug!`, `trace!`)
- Choose appropriate log levels for the context

### Dependencies

- Always check the latest crate versions before introducing a new crate
- Run `cargo outdated --root-deps-only` to ensure all dependencies are up to date

## Commit Message Style

We follow [Conventional Commits](https://www.conventionalcommits.org/). Use one of the following prefixes:

- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `refactor:` - Code refactoring (no behavior change)
- `test:` - Adding or updating tests
- `chore:` - Maintenance tasks
- `deploy:` - Deployment or infrastructure changes
- `perf:` - Performance improvement

Example:

```
feat(k8s): add pod status watcher for sandbox lifecycle

Implements a background watcher that monitors pod events and
updates sandbox state accordingly.

Closes #123
```

## Pull Request Process

1. **Fork and branch**: Create a feature branch from `main` with a descriptive name
2. **Ensure tests pass**: Run `make test` before submitting
3. **Ensure clippy is clean**: Run `cargo clippy --all-targets --all-features`
4. **Update documentation**: Update relevant docs if your change affects behavior
5. **Add a changelog entry**: Add a bullet to the `[Unreleased]` section of the appropriate `CHANGELOG.md` (root for Rust core, `sdks/python/CHANGELOG.md` for the SDK)
6. **Reference issues**: Link any related issues in the PR description
7. **Wait for review**: A maintainer will review your PR and may request changes. Once approved, it will be merged into `main`.

## Python SDK

The Python SDK is located in `sdks/python/`. Use `uv` to manage dependencies. Run `make test`, `make lint`, and `make format` to verify your changes.

## Release Process

Releases are cut by maintainers:

1. Bump versions in `Cargo.toml` workspace, `sdks/python/pyproject.toml`, and Docker image tags
2. Move `[Unreleased]` entries in CHANGELOG files into a dated version section
3. Tag the release: `git tag -s v0.X.Y`
4. CI builds and publishes Docker images to `ghcr.io/dsb/*` and the Python SDK to PyPI

## License

By contributing, you agree that your contributions will be licensed under the project's dual license (Apache 2.0 / AGPL 3.0) for core code, and MIT for the Python SDK.

Thank you for contributing to DSB! 🎉

