# Contributing to DSB

Thank you for considering contributing to DSB (Distributed Sandbox). We welcome contributions from everyone and are grateful for your time and effort.

## Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How to Contribute

### Reporting Bugs

If you find a bug, please open a GitHub issue using the bug report template. Include as much detail as possible:

- A clear description of the bug
- Steps to reproduce the issue
- Expected vs. actual behavior
- Your environment (OS, Rust version, Docker version)
- Any relevant logs or error messages

### Suggesting Features

We welcome feature suggestions. Please open a GitHub issue using the feature request template and describe:

- The problem you are trying to solve
- Your proposed solution
- Any alternatives you have considered

### Code Contributions

1. Fork the repository
2. Create a feature branch from `dev`
3. Make your changes
4. Ensure all tests pass and code standards are met
5. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.88 or later
- Docker
- (Optional) Kubernetes cluster or `kind` for K8s backend testing

### Building

```bash
cargo build --release
```

### Testing

```bash
make test
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

- Use inline test modules: `#[cfg(test)] mod tests {`
- Never use separate test files
- Test modules must be declared as `#[cfg(test)] mod name;`, never `pub mod name;`
- If a test fails, fix the underlying issue rather than ignoring it

### Configuration

- Never hardcode configuration values
- Use `dsb::config::load()` in production code
- Use `dsb::config::load_for_tests()` in test code

### Documentation

- Add documentation to all public functions
- Add markdown module documentation
- Maintain the README.md

### Logging

- Use `tracing` logs with correct levels
- Choose appropriate log levels for the context

### Dependencies

- Always check the latest crate versions before introducing a new crate
- Run `cargo outdated --root-deps-only` to ensure all dependencies are up to date

## Commit Message Style

We follow [Conventional Commits](https://www.conventionalcommits.org/). Use one of the following prefixes:

- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `refactor:` - Code refactoring
- `test:` - Adding or updating tests
- `chore:` - Maintenance tasks
- `deploy:` - Deployment or infrastructure changes

Example:

```
feat(k8s): add pod status watcher for sandbox lifecycle

Implements a background watcher that monitors pod events and
updates sandbox state accordingly.
```

## Pull Request Process

1. **Fork and branch**: Create a feature branch from `dev` with a descriptive name
2. **Ensure tests pass**: Run `make test` before submitting
3. **Ensure clippy is clean**: Run `cargo clippy --all-targets --all-features`
4. **Update documentation**: Update relevant docs if your change affects behavior
5. **Reference issues**: Link any related issues in the PR description

A maintainer will review your PR and may request changes. Once approved, it will be merged into `dev`.

Thank you for contributing to DSB!

## Python SDK
The Python SDK is located in `sdks/python/`. Use `uv` to manage dependencies. Run `make test`, `make lint`, and `make format` to verify your changes.
