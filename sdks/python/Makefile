# Makefile for DSB Python SDK
# Provides convenient commands for development, testing, and quality assurance

.PHONY: help install install-dev update-deps clean test test-unit test-integration test-all
.PHONY: test-cov lint lint-fix format check type-check security security-full
.PHONY: build publish docs docs-serve benchmark benchmark-all
.PHONY: deps-audit deps-update run-examples

# Variables
PYTHON := python3.12
VENV := .venv
SRC := src/dsb_sdk
TESTS := tests
EXAMPLES := examples
COVERAGE_MIN := 80
PORT := 8080

# Default target
help:
	@echo "DSB Python SDK - Available Commands:"
	@echo ""
	@echo "Setup:"
	@echo "  make install          - Install package dependencies"
	@echo "  make install-dev      - Install development dependencies"
	@echo "  make update-deps      - Update all dependencies to latest versions"
	@echo "  make clean            - Remove build artifacts and cache"
	@echo ""
	@echo "Testing:"
	@echo "  make test             - Run unit tests only"
	@echo "  make test-unit        - Run unit tests only (same as test)"
	@echo "  make test-integration - Run integration tests (requires DSB server)"
	@echo "  make test-e2e         - Full E2E: start services + run integration tests"
	@echo "  make test-e2e-ci      - E2E for CI: assumes fresh docker, no prompts"
	@echo "  make test-all         - Run all tests (unit + integration)"
	@echo "  make test-cov         - Run tests with coverage report"
	@echo "  make test-slow        - Run slow tests (marked with @slow)"
	@echo ""
	@echo "Code Quality:"
	@echo "  make lint             - Run linter (ruff) to check code quality"
	@echo "  make lint-fix         - Auto-fix linting issues"
	@echo "  make format           - Format code with ruff"
	@echo "  make check            - Run all quality checks (lint + type-check + security)"
	@echo "  make type-check       - Run type checker (mypy)"
	@echo ""
	@echo "Security:"
	@echo "  make security         - Run basic security checks (bandit)"
	@echo "  make security-full    - Run full security audit (bandit + safety)"
	@echo "  make deps-audit       - Audit dependencies for vulnerabilities"
	@echo "  make deps-update      - Update dependencies to latest secure versions"
	@echo ""
	@echo "Build & Release:"
	@echo "  make build            - Build distribution packages (wheel + sdist)"
	@echo "  make publish          - Publish to PyPI (requires API token)"
	@echo "  make publish-test     - Publish to Test PyPI"
	@echo ""
	@echo "Documentation:"
	@echo "  make docs             - Generate documentation (if Sphinx configured)"
	@echo "  make docs-serve       - Serve documentation locally"
	@echo ""
	@echo "Performance:"
	@echo "  make benchmark        - Run performance benchmarks"
	@echo "  make benchmark-all    - Run all performance tests"
	@echo ""
	@echo "Development:"
	@echo "  make run-examples     - Run all example scripts"
	@echo "  make dev-server       - Start DSB server for integration testing"
	@echo ""
	@echo "Quick Reference:"
	@echo "  make all              - Run full CI pipeline (install -> test -> check)"
	@echo "  make pre-commit       - Quick check before git commit (format + lint + type-check)"

# ============================================================================
# Setup Commands
# ============================================================================

install:
	@echo "Installing package dependencies..."
	pip install -e .

install-dev:
	@echo "Installing development dependencies..."
	pip install -e ".[dev]"

update-deps:
	@echo "Updating all dependencies to latest versions..."
	pip install --upgrade pip
	pip install --upgrade -e ".[dev]"
	@echo "Dependencies updated. Run 'make deps-audit' to check for vulnerabilities."

clean:
	@echo "Cleaning build artifacts..."
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "*.egg-info" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".mypy_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".ruff_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "htmlcov" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".coverage" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true
	rm -rf dist/ build/ *.egg 2>/dev/null || true
	@echo "Clean complete!"

# ============================================================================
# Testing Commands
# ============================================================================

# Docker Compose configuration (relative to SDK directory)
DOCKER_COMPOSE_FILE := ../../docker/docker-compose.yml
DSB_API_URL := http://localhost:$(PORT)
DSB_API_KEY ?= dev-bootstrap-key-CHANGE-ME
TEST_ENV := PYTHONPATH=src DSB_API_URL=$(DSB_API_URL) DSB_API_KEY=$(DSB_API_KEY)

test test-unit:
	@echo "Running unit tests..."
	pytest $(TESTS)/unit/ -v --tb=short

test-integration:
	@echo "Running integration tests (requires DSB server on port $(PORT))..."
	@if curl -s http://localhost:$(PORT)/health > /dev/null; then \
		$(TEST_ENV) pytest $(TESTS)/integration/ -v --tb=short -m "requires_server"; \
	else \
		echo "ERROR: DSB server not running on port $(PORT)"; \
		echo "Start it with: make test-e2e (auto-starts services) or make dev-server"; \
		exit 1; \
	fi

# Internal: Wait for DSB server to be healthy
_wait-for-server:
	@echo "Waiting for DSB server to be healthy..."
	@for i in 1 2 3 4 5 6 7 8 9 10; do \
		if curl -s http://localhost:$(PORT)/health > /dev/null 2>&1; then \
			echo "DSB server is ready!"; \
			exit 0; \
		fi; \
		echo "  Attempt $$i/10 - waiting 5 seconds..."; \
		sleep 5; \
	done; \
	echo "ERROR: DSB server failed to become healthy within 50 seconds"; \
	exit 1

test-e2e:
	@echo "=========================================="
	@echo "Running End-to-End Tests"
	@echo "=========================================="
	@echo ""
	@echo "This will:"
	@echo "  1. Start Docker Compose services (if not running)"
	@echo "  2. Wait for services to be healthy"
	@echo "  3. Run all integration tests"
	@echo ""
	@read -p "Continue? [y/N] " -n 1 -r; \
	echo; \
	if [[ ! $$REPLY =~ ^[Yy]$$ ]]; then \
		echo "Aborted."; \
		exit 0; \
	fi
	@echo ""
	@echo "Step 1/3: Checking Docker Compose services..."
	@if [ -f $(DOCKER_COMPOSE_FILE) ]; then \
		if docker compose -f $(DOCKER_COMPOSE_FILE) ps | grep -q "dsb-server"; then \
			echo "  Docker Compose services already running"; \
		else \
			echo "  Starting Docker Compose services..."; \
			docker compose -f $(DOCKER_COMPOSE_FILE) up -d; \
		fi; \
	else \
		echo "ERROR: Docker Compose file not found at $(DOCKER_COMPOSE_FILE)"; \
		echo "Make sure you're running from within the SDK directory"; \
		exit 1; \
	fi
	@$(MAKE) _wait-for-server
	@echo ""
	@echo "Step 3/3: Running integration tests..."
	@echo "=========================================="
	@$(TEST_ENV) pytest $(TESTS)/integration/ -v --tb=short -m "requires_server" || TEST_EXIT=$$?; \
	echo ""; \
	echo "=========================================="; \
	echo "E2E Test Run Complete"; \
	echo "=========================================="; \
	exit $${TEST_EXIT:-0}

test-e2e-ci:
	@echo "=========================================="
	@echo "Running E2E Tests (CI Mode)"
	@echo "=========================================="
	@echo ""
	@echo "Step 1/3: Starting Docker Compose services..."
	@docker compose -f $(DOCKER_COMPOSE_FILE) up -d --wait
	@echo "  Services started"
	@echo ""
	@$(MAKE) _wait-for-server
	@echo ""
	@echo "Step 3/3: Running integration tests..."
	@echo "=========================================="
	@$(TEST_ENV) pytest $(TESTS)/integration/ -v --tb=short -m "requires_server" || TEST_EXIT=$$?; \
	echo ""; \
	echo "=========================================="; \
	echo "E2E Test Run Complete (CI Mode)"; \
	echo "=========================================="; \
	exit $${TEST_EXIT:-0}

test-all:
	@echo "Running all tests..."
	pytest $(TESTS)/ -v --tb=short

test-cov:
	@echo "Running tests with coverage..."
	pytest $(TESTS)/ \
		--cov=$(SRC) \
		--cov-report=html \
		--cov-report=term-missing \
		--cov-fail-under=$(COVERAGE_MIN) \
		-v
	@echo ""
	@echo "Coverage report generated in htmlcov/index.html"
	@echo "Minimum coverage required: $(COVERAGE_MIN)%"

test-slow:
	@echo "Running slow tests..."
	pytest $(TESTS)/ -v -m "slow" --tb=short

# ============================================================================
# Code Quality Commands
# ============================================================================

lint:
	@echo "Running linter (ruff)..."
	ruff check $(SRC)/ $(TESTS)/

lint-fix:
	@echo "Auto-fixing linting issues..."
	ruff check $(SRC)/ $(TESTS)/ --fix

format:
	@echo "Formatting code with ruff..."
	ruff format $(SRC)/ $(TESTS)/ $(EXAMPLES)/

check: lint type-check security
	@echo "All quality checks passed!"

type-check:
	@echo "Running type checker (mypy)..."
	mypy $(SRC)/

# ============================================================================
# Security Commands
# ============================================================================

security:
	@echo "Running security checks (bandit)..."
	bandit -r $(SRC)/ -f screen -ll

security-full: security deps-audit
	@echo "Full security audit complete!"

deps-audit:
	@echo "Auditing dependencies for vulnerabilities..."
	@echo "Checking with pip-audit..."
	@which pip-audit > /dev/null || pip install pip-audit
	pip-audit || true
	@echo ""
	@echo "Checking with safety..."
	@which safety > /dev/null || pip install safety
	safety check --json || true
	@echo ""
	@echo "Dependency audit complete. Review results above."

deps-update:
	@echo "Updating dependencies to latest secure versions..."
	pip install --upgrade pip
	pip list --outdated
	@echo ""
	@echo "To update a specific package: pip install --upgrade <package_name>"
	@echo "After updating, run 'make deps-audit' to verify security."

# ============================================================================
# Build & Release Commands
# ============================================================================

build:
	@echo "Building distribution packages..."
	rm -rf dist/
	python -m build
	@echo "Build complete! Packages in dist/:"
	@ls -lh dist/

publish: build
	@echo "Publishing to PyPI..."
	@echo "Make sure you have:"
	@echo "  1. Updated version in pyproject.toml"
	@echo "  2. Updated CHANGELOG.md"
	@echo "  3. Run 'make test-all' successfully"
	@echo "  4. Set TWINE_USERNAME and TWINE_PASSWORD environment variables"
	@read -p "Continue? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		twine upload dist/*; \
	else \
		echo "Publish cancelled."; \
	fi

publish-test: build
	@echo "Publishing to Test PyPI..."
	twine upload --repository testpypi dist/*

# ============================================================================
# Documentation Commands
# ============================================================================

docs:
	@echo "Generating documentation..."
	@echo "Note: Sphinx documentation not yet configured."
	@echo "To enable: pip install sphinx sphinx-rtd-theme"
	@echo "Then create docs/ directory with conf.py"

docs-serve: docs
	@echo "Serving documentation at http://localhost:8000"
	@echo "Note: Requires Sphinx to be configured"
	@cd docs && python -m http.server 8000

# ============================================================================
# Performance Commands
# ============================================================================

benchmark:
	@echo "Running performance benchmarks..."
	@which pytest-benchmark > /dev/null || pip install pytest-benchmark
	pytest tests/benchmarks/ -v --benchmark-only || echo "No benchmarks found. Create tests/benchmarks/ directory."

benchmark-all:
	@echo "Running all performance tests..."
	@which pytest-benchmark > /dev/null || pip install pytest-benchmark
	pytest tests/benchmarks/ -v --benchmark-only --benchmark-autosave || echo "No benchmarks found."

# ============================================================================
# Development Commands
# ============================================================================

run-examples:
	@echo "Running example scripts..."
	@for script in $(EXAMPLES)/*.py; do \
		if [ -f "$$script" ]; then \
			echo "Running $$script..."; \
			python "$$script" || echo "Failed: $$script"; \
		fi; \
	done

dev-server:
	@echo "Starting DSB development server on port $(PORT)..."
	@echo "This requires the DSB server to be installed separately."
	@echo "See: https://github.com/xieyuanpeng/dsb"
	@echo ""
	@echo "Typical commands:"
	@echo "  cd ../../ && cargo run -- --port $(PORT)"

# ============================================================================
# CI/CD Pipeline Commands
# ============================================================================

all: install-dev test-all check
	@echo "Full CI pipeline complete!"

pre-commit: format lint type-check
	@echo "Pre-commit checks passed!"

# ============================================================================
# Utility Commands
# ============================================================================

show-versions:
	@echo "Python version:"
	@$(PYTHON) --version
	@echo ""
	@echo "Package versions:"
	@pip list | grep -E "(httpx|pydantic|sse-starlette|websocket-client|anyio)"

stats:
	@echo "Code statistics:"
	@echo "  Python files: $$(find $(SRC) -name '*.py' | wc -l | tr -d ' ')"
	@echo "  Test files: $$(find $(TESTS) -name 'test_*.py' | wc -l | tr -d ' ')"
	@echo "  Lines of code: $$(find $(SRC) -name '*.py' -exec wc -l {} + | tail -1 | awk '{print $$1}')"
	@echo "  Test lines: $$(find $(TESTS) -name 'test_*.py' -exec wc -l {} + | tail -1 | awk '{print $$1}')"

# ============================================================================
# Help (Default)
# ============================================================================

.DEFAULT_GOAL := help
