# DSB Makefile - Unified build and test commands
# Configuration managed via .env (see: make config-default / make config-china)

.PHONY: build test test-quick test-unit test-integration test-python test-agent test-agent-k8s test-mcp test-e2e test-clean
.PHONY: test-k8s-e2e test-k8s-agent-e2e test-k8s-e2e-cleanup test-k8s-e2e-delete-cluster
.PHONY: dc-build dc-up dc-down dc-restart dc-logs
.PHONY: base-images-build sandbox sandbox-slim
.PHONY: clean clean-docker clean-dangling-images
.PHONY: check fix-all lint audit
.PHONY: config-default config-china help
.PHONY: _ensure-build _ensure-sandbox-image

# Configuration
DOCKER_DIR := docker
GIT_BRANCH := $(shell git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
TAG ?= latest
REGISTRY_PREFIX ?= docker.io/

# All images built by this Makefile
BASE_IMAGES := rust-base python-base node-base runtime-base sandbox-base
SANDBOX_IMAGES := sandbox sandbox-slim
SERVICE_IMAGES := dashboard mcp-server server ssh-gateway
ALL_IMAGES := $(BASE_IMAGES) $(SANDBOX_IMAGES) $(SERVICE_IMAGES)

# Test commands (extracted to avoid duplication)
TEST_UNIT_CMD := cargo test --lib --workspace --exclude dsb-agent-tester
TEST_INTEGRATION_CMD := cargo test --workspace --all-targets --exclude dsb-agent-tester
# Skip test_dsb_stack_lifecycle: it starts its own DSBStack (docker compose up)
# which conflicts with the already-running test server on the same port.
TEST_AGENT_CMD := cargo test -p dsb-agent-tester -- --nocapture --skip test_dsb_stack_lifecycle
TEST_PYTHON_UNIT_CMD := cd /workspace/sdks/python && pytest tests/unit -v -n auto
TEST_PYTHON_INTEGRATION_CMD := cd /workspace/sdks/python && pytest tests/integration -v -m 'not requires_databend'
TEST_PYTHON_CMD := $(TEST_PYTHON_UNIT_CMD) && $(TEST_PYTHON_INTEGRATION_CMD)
TEST_RUNNER_RUST_PREFIX := export PATH=/usr/local/cargo/bin:$$PATH &&
TEST_DOCKER_COMPOSE := TAG=$(TAG) docker compose -f $(DOCKER_DIR)/docker-compose.test.yml

# Default target
.DEFAULT_GOAL := help

# =============================================================================
# Help
# =============================================================================

help:
	@echo "DSB (Docker Sandbox) - Development Commands"
	@echo ""
	@echo "Configuration:"
	@echo "  make config-default    - Configure for International usage"
	@echo "  make config-china      - Configure for China usage (mirrors)"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build             - Build workspace (cargo)"
	@echo "  make test              - Run all tests (Docker)"
	@echo "  make test-quick        - Quick unit tests (cargo, no Docker)"
	@echo "  make test-unit         - Unit tests (Docker)"
	@echo "  make test-integration  - Integration tests (Docker)"
	@echo "  make test-python       - Python SDK tests (Docker)"
	@echo "  make test-agent        - Agent Tester tests (Docker)"
	@echo "  make test-agent-k8s    - Agent Tester vs kind + Helm (needs test-k8s-e2e first)"
	@echo "  make test-k8s-agent-e2e - sandbox image + test-k8s-e2e + test-agent-k8s (full kind E2E)"
	@echo "  make test-mcp          - MCP Server tests (isolated)"
	@echo "  make test-e2e          - Dashboard E2E tests (Playwright, Docker)"
	@echo "  make test-k8s-e2e      - kind + Helm + FEATURES=kubernetes (see Makefile vars)"
	@echo "  make test-k8s-e2e-cleanup - Uninstall Helm release and e2e namespaces"
	@echo "  make test-clean        - Remove test containers and volumes (fresh start)"
	@echo ""
	@echo "Development:"
	@echo "  make dc-build          - Build, tag, and push ALL images (uses REGISTRY_PREFIX, TAG)"
	@echo "  make dc-build-sandbox - Build, tag, and push ALL sandbox images only"
	@echo "  make dc-up             - Start services"
	@echo "  make dc-down           - Stop services"
	@echo "  make dc-logs           - View logs"
	@echo ""
	@echo "Images:"
	@echo "  make base-images-build - Build all base images"
	@echo "  make sandbox           - Build sandbox image"
	@echo "  make sandbox-slim      - Build sandbox-slim image"
	@echo ""
	@echo "Variables:"
	@echo "  REGISTRY_PREFIX=docker.io/  TAG=latest"
	@echo ""
	@echo "Code Quality:"
	@echo "  make check             - Check compilation"
	@echo "  make lint              - Run clippy"
	@echo "  make audit             - Run cargo audit (security vulnerability check)"
	@echo "  make fix-all           - Auto-fix clippy + format"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean             - Clean cargo build"
	@echo "  make clean-docker      - Remove dangling images, stopped containers,"
	@echo "                           and unused networks"
	@echo "  make clean-dangling-images  - Remove dangling Docker images only"

# =============================================================================
# Configuration
# =============================================================================

config-default:
	@cp docker/.env.example .env
	@echo "✅ Configured for Default (International) region"

config-china:
	@cp docker/.env.china.example .env
	@echo "✅ Configured for China region"

_check-config:
	@if [ ! -f .env ]; then \
		echo "❌ No .env file. Run 'make config-default' or 'make config-china' first."; \
		exit 1; \
	fi

# =============================================================================
# Build & Development
# =============================================================================

build: _check-config
	cargo build --workspace

# =============================================================================
# Tests
# =============================================================================

test: _check-config _test-setup
	@TEST_START=$$(date +%s); \
	$(MAKE) --no-print-directory _test-run; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	TEST_END=$$(date +%s); \
	ELAPSED=$$((TEST_END - TEST_START)); \
	MINUTES=$$((ELAPSED / 60)); \
	SECONDS=$$((ELAPSED % 60)); \
	if [ $$EXIT_CODE -eq 0 ]; then \
		echo "✅ All tests passed in $${MINUTES}m $${SECONDS}s"; \
	else \
		echo "❌ Tests failed after $${MINUTES}m $${SECONDS}s"; \
	fi; \
	exit $$EXIT_CODE

test-quick:
	@echo "🧪 Quick unit tests..."
	cargo test --lib --workspace

test-unit: _check-config _ensure-build
	@echo "🧪 Unit tests..."
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d postgres-test test-runner
	@docker exec dsb-test-runner bash -lc "$(TEST_RUNNER_RUST_PREFIX) cd /workspace && $(TEST_UNIT_CMD)"; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	exit $$EXIT_CODE

test-integration: _check-config _ensure-build
	@echo "🧪 Integration tests..."
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d dsb-server-test postgres-test test-runner
	@docker exec dsb-test-runner bash -lc "$(TEST_RUNNER_RUST_PREFIX) cd /workspace && $(TEST_INTEGRATION_CMD)"; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	exit $$EXIT_CODE

test-agent-k8s: _check-config
	@echo "🧪 Agent Tester (MCP) against kind cluster..."
	@K8S_AGENT_SANDBOX_IMAGE=$(K8S_AGENT_SANDBOX_IMAGE) \
		K8S_E2E_NAMESPACE=$(K8S_E2E_NAMESPACE) \
		K8S_E2E_CLUSTER_NAME=$(K8S_E2E_CLUSTER_NAME) \
		TAG=$(TAG) \
		DSB_MCP_SERVER_BIN=$(DSB_MCP_SERVER_BIN) \
		DSB_AGENT_TEST_THREADS=$(K8S_AGENT_TEST_THREADS) \
		bash scripts/run-agent-tester-k8s.sh

test-agent: _check-config _ensure-build _ensure-sandbox-image
	@echo "🧪 Agent Tester tests..."
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d postgres-test searxng-test dsb-server-test dsb-mcp-server-test test-runner
	@docker exec \
		-e DSB_MCP_URL=http://dsb-mcp-server-test:3000/mcp \
		-e DSB_TEST_SANDBOX_IMAGE=dsb/sandbox:$(TAG) \
		dsb-test-runner bash -lc "$(TEST_RUNNER_RUST_PREFIX) cd /workspace && $(TEST_AGENT_CMD)"; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	exit $$EXIT_CODE

test-python: _check-config _ensure-build
	@echo "🧪 Python tests..."
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d dsb-server-test postgres-test python-test-runner
	@docker exec -e DSB_API_URL=http://dsb-server-test:8080 \
		-e DSB_API_KEY=test-admin-key-for-testing-only \
		dsb-python-test-runner bash -c "$(TEST_PYTHON_CMD)"; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	exit $$EXIT_CODE

test-mcp: _check-config _ensure-build
	@echo "🧪 MCP Server tests (isolated)..."
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d postgres-test searxng-test dsb-server-test dsb-mcp-server-test test-runner
	@docker exec dsb-test-runner bash -lc "$(TEST_RUNNER_RUST_PREFIX) cd /workspace && cargo test -p dsb-mcp-server"; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	exit $$EXIT_CODE

test-e2e: _check-config _ensure-build
	@echo "🧪 Dashboard E2E tests (base path: /dsb)..."
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d postgres-test dsb-server-test dashboard-test e2e-proxy e2e-runner
	@docker exec dsb-e2e-runner npx playwright test; \
	EXIT_CODE=$$?; \
	$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true; \
	exit $$EXIT_CODE

_test-setup: _ensure-build _ensure-sandbox-image
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true
	@$(TEST_DOCKER_COMPOSE) up -d postgres-test searxng-test dsb-server-test dsb-mcp-server-test test-runner python-test-runner dashboard-test e2e-proxy e2e-runner
	@sleep 5

_test-run:
	@echo "========================================"
	@echo "🧪 Running Rust + Python Unit tests in parallel"
	@echo "========================================"
	@# Python unit tests run in background; Rust tests run in foreground.
	@# Python integration tests start only AFTER Rust finishes to avoid
	@# concurrent sandbox creation exhausting Docker resources.
	@PHASE_START=$$(date +%s); \
	docker exec -e DSB_API_URL=http://dsb-server-test:8080 -e DSB_API_KEY=test-admin-key-for-testing-only \
		dsb-python-test-runner bash -c "$(TEST_PYTHON_UNIT_CMD)" > /tmp/dsb-python-unit-output.log 2>&1 & \
	PYTHON_UNIT_PID=$$!; \
	echo "  ⏳ Python unit tests running in background (PID $$PYTHON_UNIT_PID)..."; \
	echo "  ⏳ Rust tests running in foreground..."; \
	echo "========================================"; \
	echo "🧪 Rust Unit + Integration Tests"; \
	echo "========================================"; \
	docker exec dsb-test-runner bash -lc "$(TEST_RUNNER_RUST_PREFIX) cd /workspace && cargo test --workspace --all-targets --exclude dsb-agent-tester"; \
	RUST_EXIT=$$?; \
	RUST_ELAPSED=$$(( $$(date +%s) - PHASE_START )); \
	echo "  ⏱  Rust tests: $${RUST_ELAPSED}s"; \
	echo ""; \
	echo "========================================"; \
	echo "🧪 Waiting for Python Unit Tests..."; \
	echo "========================================"; \
	wait $$PYTHON_UNIT_PID; \
	PYTHON_UNIT_EXIT=$$?; \
	cat /tmp/dsb-python-unit-output.log; \
	rm -f /tmp/dsb-python-unit-output.log; \
	if [ $$RUST_EXIT -ne 0 ]; then echo "❌ Rust tests failed"; exit $$RUST_EXIT; fi; \
	if [ $$PYTHON_UNIT_EXIT -ne 0 ]; then echo "❌ Python unit tests failed"; exit $$PYTHON_UNIT_EXIT; fi; \
	echo ""; \
	echo "========================================"; \
	echo "🧪 Python Integration Tests"; \
	echo "========================================"; \
	INT_START=$$(date +%s); \
	docker exec -e DSB_API_URL=http://dsb-server-test:8080 -e DSB_API_KEY=test-admin-key-for-testing-only \
		dsb-python-test-runner bash -c "$(TEST_PYTHON_INTEGRATION_CMD)"; \
	PYTHON_INT_EXIT=$$?; \
	PYTHON_INT_ELAPSED=$$(( $$(date +%s) - INT_START )); \
	echo "  ⏱  Python integration tests: $${PYTHON_INT_ELAPSED}s"; \
	if [ $$PYTHON_INT_EXIT -ne 0 ]; then echo "❌ Python integration tests failed"; exit $$PYTHON_INT_EXIT; fi; \
	echo ""; \
	echo "========================================"; \
	echo "🧪 Agent Tester Tests"; \
	echo "========================================"; \
	AGENT_START=$$(date +%s); \
	docker exec \
		-e DSB_MCP_URL=http://dsb-mcp-server-test:3000/mcp \
		-e DSB_TEST_SANDBOX_IMAGE=dsb/sandbox:$(TAG) \
		dsb-test-runner bash -lc "$(TEST_RUNNER_RUST_PREFIX) cd /workspace && $(TEST_AGENT_CMD)" || exit 1; \
	AGENT_ELAPSED=$$(( $$(date +%s) - AGENT_START )); \
	echo "  ⏱  Agent tests: $${AGENT_ELAPSED}s"; \
	echo ""; \
	echo "========================================"; \
	echo "🧪 Dashboard E2E Tests"; \
	echo "========================================"; \
	E2E_START=$$(date +%s); \
	docker exec dsb-e2e-runner npx playwright test || exit 1; \
	E2E_ELAPSED=$$(( $$(date +%s) - E2E_START )); \
	echo "  ⏱  E2E tests: $${E2E_ELAPSED}s"

_test-cleanup:
	@$(TEST_DOCKER_COMPOSE) down 2>/dev/null || true

test-clean:
	@echo "🧹 Cleaning test containers and volumes..."
	@$(TEST_DOCKER_COMPOSE) down -v 2>/dev/null || true

# =============================================================================
# Docker Compose
# =============================================================================

dc-build: _check-config
	@BUILD_START=$$(date +%s); \
	echo "================================================================"; \
	echo "🔨 Building ALL images with TAG=$(TAG) REGISTRY_PREFIX=$(REGISTRY_PREFIX)"; \
	echo "================================================================"; \
	echo ""; \
	echo "📦 Step 1/4 — Base images (parallel: 5 independent builds)..."; \
	$(MAKE) --no-print-directory -j5 rust-base python-base node-base runtime-base sandbox-base || exit 1; \
	echo ""; \
	echo "📦 Step 2/4 — Sandbox + Service images (parallel groups)..."; \
	echo "  ↳ Sandbox chain: sandbox+sandbox-slim"; \
	echo "  ↳ Services: dashboard, mcp-server, server, ssh-gateway"; \
	( \
		$(MAKE) --no-print-directory -j2 sandbox sandbox-slim TAG=$(TAG) \
	) & \
	SANDBOX_PID=$$!; \
	TAG=$(TAG) docker compose -f $(DOCKER_DIR)/docker-compose.yml build & \
	SERVICES_PID=$$!; \
	wait $$SERVICES_PID; SERVICES_EXIT=$$?; \
	wait $$SANDBOX_PID; SANDBOX_EXIT=$$?; \
	if [ $$SERVICES_EXIT -ne 0 ]; then echo "❌ Service images failed"; exit 1; fi; \
	if [ $$SANDBOX_EXIT -ne 0 ]; then echo "❌ Sandbox images failed"; exit 1; fi; \
	echo ""; \
	echo "📦 Step 3/4 — Tagging all images with $(REGISTRY_PREFIX)..."; \
	for img in $(ALL_IMAGES); do \
		echo "  Tagging dsb/$$img:$(TAG) → $(REGISTRY_PREFIX)dsb/$$img:$(TAG)"; \
		docker tag dsb/$$img:$(TAG) $(REGISTRY_PREFIX)dsb/$$img:$(TAG); \
	done; \
	echo ""; \
	echo "📦 Step 4/4 — Pushing all images (parallel: $(words $(ALL_IMAGES)) concurrent pushes)..."; \
	PUSH_FAIL=$$(mktemp); \
	for img in $(ALL_IMAGES); do \
		echo "  Pushing $(REGISTRY_PREFIX)dsb/$$img:$(TAG)"; \
		(docker push $(REGISTRY_PREFIX)dsb/$$img:$(TAG) || echo "$$img" >> $$PUSH_FAIL) & \
	done; \
	wait; \
	if [ -s $$PUSH_FAIL ]; then \
		echo "❌ Failed pushes: $$(cat $$PUSH_FAIL)"; \
		rm -f $$PUSH_FAIL; \
		exit 1; \
	fi; \
	rm -f $$PUSH_FAIL; \
	BUILD_END=$$(date +%s); \
	ELAPSED=$$((BUILD_END - BUILD_START)); \
	MINUTES=$$((ELAPSED / 60)); \
	SECONDS=$$((ELAPSED % 60)); \
	echo ""; \
	echo "================================================================"; \
	echo "✅ All images built, tagged, and pushed in $${MINUTES}m $${SECONDS}s!"; \
	echo "================================================================"

dc-tag-push: _check-config
	@echo "================================================================"; \
	echo "🏷️ Tagging and pushing ALL images with TAG=$(TAG) REGISTRY_PREFIX=$(REGISTRY_PREFIX)"; \
	echo "================================================================"; \
	echo ""; \
	echo "📦 Step 1/2 — Tagging all images with $(REGISTRY_PREFIX)..."; \
	for img in $(ALL_IMAGES); do \
		echo "  Tagging dsb/$$img:$(TAG) → $(REGISTRY_PREFIX)dsb/$$img:$(TAG)"; \
		docker tag dsb/$$img:$(TAG) $(REGISTRY_PREFIX)dsb/$$img:$(TAG); \
	done; \
	echo ""; \
	echo "📦 Step 2/2 — Pushing all images (parallel: $(words $(ALL_IMAGES)) concurrent pushes)..."; \
	PUSH_FAIL=$$(mktemp); \
	for img in $(ALL_IMAGES); do \
		echo "  Pushing $(REGISTRY_PREFIX)dsb/$$img:$(TAG)"; \
		(docker push $(REGISTRY_PREFIX)dsb/$$img:$(TAG) || echo "$$img" >> $$PUSH_FAIL) & \
	done; \
	wait; \
	if [ -s $$PUSH_FAIL ]; then \
		echo "❌ Failed pushes: $$(cat $$PUSH_FAIL)"; \
		rm -f $$PUSH_FAIL; \
		exit 1; \
	fi; \
	rm -f $$PUSH_FAIL; \
	echo ""; \
	echo "================================================================"; \
	echo "✅ All images tagged and pushed successfully!"; \
	echo "================================================================"

dc-up: _check-config
	@echo "🚀 Starting services..."
	TAG=$(TAG) docker compose -f $(DOCKER_DIR)/docker-compose.yml up -d
	@echo "✅ Services started:"
	@echo "  Dashboard: http://localhost:3001"
	@echo "  API:       http://localhost:8080"

dc-down: _check-config
	docker compose -f $(DOCKER_DIR)/docker-compose.yml down

dc-restart: _check-config
	docker compose -f $(DOCKER_DIR)/docker-compose.yml restart

dc-logs: _check-config
	docker compose -f $(DOCKER_DIR)/docker-compose.yml logs --tail=100 -f

dc-build-sandbox: _check-config
	@BUILD_START=$$(date +%s); \
	echo "================================================================"; \
	echo "🔨 Building sandbox images with TAG=$(TAG) REGISTRY_PREFIX=$(REGISTRY_PREFIX)"; \
	echo "================================================================"; \
	echo ""; \
	echo "📦 Step 1/3 — sandbox-base (required by all variants)..."; \
	$(MAKE) --no-print-directory sandbox-base || exit 1; \
	echo ""; \
	echo "📦 Step 2/3 — Sandbox variants (parallel: 2 builds)..."; \
	$(MAKE) --no-print-directory -j2 sandbox sandbox-slim TAG=$(TAG) || exit 1; \
	echo ""; \
	echo "📦 Step 3/3 — Tagging and pushing sandbox images..."; \
	PUSH_FAIL=$$(mktemp); \
	for img in $(SANDBOX_IMAGES); do \
		echo "  Tagging dsb/$$img:$(TAG) → $(REGISTRY_PREFIX)dsb/$$img:$(TAG)"; \
		docker tag dsb/$$img:$(TAG) $(REGISTRY_PREFIX)dsb/$$img:$(TAG) || echo "$$img" >> $$PUSH_FAIL; \
		echo "  Pushing $(REGISTRY_PREFIX)dsb/$$img:$(TAG)"; \
		(docker push $(REGISTRY_PREFIX)dsb/$$img:$(TAG) || echo "$$img" >> $$PUSH_FAIL) & \
	done; \
	wait; \
	if [ -s $$PUSH_FAIL ]; then \
		echo "❌ Failed: $$(cat $$PUSH_FAIL)"; \
		rm -f $$PUSH_FAIL; \
		exit 1; \
	fi; \
	rm -f $$PUSH_FAIL; \
	BUILD_END=$$(date +%s); \
	ELAPSED=$$((BUILD_END - BUILD_START)); \
	MINUTES=$$((ELAPSED / 60)); \
	SECONDS=$$((ELAPSED % 60)); \
	echo ""; \
	echo "================================================================"; \
	echo "✅ Sandbox images built and pushed in $${MINUTES}m $${SECONDS}s!"; \
	echo "================================================================"

# =============================================================================
# Images
# =============================================================================

base-images-build: _check-config rust-base python-base node-base runtime-base sandbox-base

rust-base python-base node-base runtime-base sandbox-base:
	@echo "🔨 Building $(@)..."
	TAG=$(TAG) docker compose -f $(DOCKER_DIR)/docker-compose.yml build --no-cache $(@)

sandbox: _check-config
	@echo "🔨 Building sandbox..."
	cd $(DOCKER_DIR)/images/sandbox && \
		docker build --build-arg BASE_IMAGE_TAG=$(TAG) \
			$$(grep -E "^(DOCKER_REGISTRY|DEBIAN_MIRROR|PYPI_MIRROR)" ../../../.env 2>/dev/null | sed 's/^/--build-arg /') \
			-t dsb/sandbox:$(TAG) .

sandbox-slim: _check-config
	@echo "🔨 Building sandbox-slim..."
	cd $(DOCKER_DIR)/images/sandbox-slim && \
		docker build --build-arg BASE_IMAGE_TAG=$(TAG) \
			$$(grep -E "^(DOCKER_REGISTRY|DEBIAN_MIRROR|PYPI_MIRROR)" ../../../.env 2>/dev/null | sed 's/^/--build-arg /') \
			-t dsb/sandbox-slim:$(TAG) .


# =============================================================================
# Code Quality
# =============================================================================

check:
	cargo check --workspace --all-targets

lint:
	cargo clippy --workspace --all-targets -- -D warnings

audit:
	@echo "🔒 Running cargo audit..."
	@cargo audit

fix-all:
	@echo "🔧 Auto-fixing..."
	@cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged -- -D warnings 2>/dev/null || true
	@cargo fmt --all
	@cargo check --workspace --all-targets
	@echo "✅ Done!"

# =============================================================================
# Cleanup
# =============================================================================

clean:
	cargo clean

clean-dangling-images:
	@echo "🗑️ Removing dangling images..."
	@docker images --filter "dangling=true" -q | xargs -r docker rmi 2>/dev/null || true
	@echo "✅ Done"

clean-docker:
	@echo "🗑️ Cleaning up Docker resources..."
	@echo "  Removing dangling images..."
	@docker images --filter "dangling=true" -q | xargs -r docker rmi 2>/dev/null || true
	@echo "  Removing stopped containers..."
	@docker container prune -f 2>/dev/null || true
	@echo "  Removing unused networks..."
	@docker network prune -f 2>/dev/null || true
	@echo "✅ Done"

# =============================================================================
# Internal
# =============================================================================

_ensure-build:
	@TAG=$(TAG) DOCKER_REGISTRY= docker compose -f $(DOCKER_DIR)/docker-compose.test.yml build dsb-server-test dsb-mcp-server-test test-runner python-test-runner dashboard-test e2e-runner

_ensure-sandbox-image:
	@$(MAKE) sandbox TAG=$(TAG)

# =============================================================================
# Kubernetes E2E Testing (kind cluster)
# =============================================================================

K8S_E2E_CLUSTER_NAME ?= dsb-e2e
K8S_E2E_NAMESPACE ?= dsb-system
K8S_E2E_SANDBOX_NAMESPACE ?= dsb-sandboxes
K8S_E2E_HELM_DIR := deployment/helm/dsb
K8S_E2E_KIND_CONFIG := /tmp/kind-config-dsb-e2e.yaml
K8S_E2E_POSTGRES_YAML := deployment/k8s-e2e/postgres.yaml
K8S_E2E_PG_PASS_FILE := /tmp/dsb-k8s-e2e.postgres.pass
# Override with empty to reuse Docker build cache: make test-k8s-e2e K8S_E2E_DOCKER_BUILD_FLAGS=
K8S_E2E_DOCKER_BUILD_FLAGS ?= --no-cache
# Sandbox image for agent-tester / kind load (match Helm config.docker.defaultImage — full dsb/sandbox for E2E)
K8S_AGENT_SANDBOX_IMAGE ?= dsb/sandbox:latest
# Limit concurrent agent-tester tests (default 2) so kind is not flooded with sandbox creates
K8S_AGENT_TEST_THREADS ?= 2
# kind nodes are small: lower sandbox *requests* so many pods can schedule (Helm wires these via DSB_SANDBOX__KUBERNETES__RESOURCE_DEFAULTS__*)
K8S_E2E_SANDBOX_CPU_REQUEST ?= 250m
K8S_E2E_SANDBOX_MEMORY_REQUEST ?= 512Mi
# Pod-ready wait after scheduling (large images may still need several minutes on first pull)
K8S_E2E_POD_READY_TIMEOUT_SECS ?= 600

$(K8S_E2E_KIND_CONFIG):
	@printf '%s\n' \
		'kind: Cluster' \
		'apiVersion: kind.x-k8s.io/v1alpha4' \
		'nodes:' \
		'- role: control-plane' \
		'  kubeadmConfigPatches:' \
		'  - |' \
		'    kind: InitConfiguration' \
		'    nodeRegistration:' \
		'      kubeletExtraArgs:' \
		'        node-labels: "dsb.io/sandbox-node=true"' \
		'  extraPortMappings:' \
		'  - containerPort: 80' \
		'    hostPort: 80' \
		'    protocol: TCP' \
		'  - containerPort: 443' \
		'    hostPort: 443' \
		'    protocol: TCP' \
		'- role: worker' \
		'  kubeadmConfigPatches:' \
		'  - |' \
		'    kind: JoinConfiguration' \
		'    nodeRegistration:' \
		'      kubeletExtraArgs:' \
		'        node-labels: "dsb.io/sandbox-node=true"' \
		'- role: worker' \
		'  kubeadmConfigPatches:' \
		'  - |' \
		'    kind: JoinConfiguration' \
		'    nodeRegistration:' \
		'      kubeletExtraArgs:' \
		'        node-labels: "dsb.io/sandbox-node=true"' \
		> $(K8S_E2E_KIND_CONFIG)

test-k8s-e2e: _check-config $(K8S_E2E_KIND_CONFIG)
	@echo "========================================"
	@echo "Kubernetes E2E Tests on kind"
	@echo "========================================"
	@echo ""
	@echo "Step 1/10 — Ensure kind cluster..."
	@if ! kind get clusters 2>/dev/null | grep -q "^$(K8S_E2E_CLUSTER_NAME)$$"; then \
		echo "  Creating kind cluster..."; \
		kind create cluster --name $(K8S_E2E_CLUSTER_NAME) --config $(K8S_E2E_KIND_CONFIG); \
	else \
		echo "  Cluster already exists, using it..."; \
	fi
	@kubectl config use-context kind-$(K8S_E2E_CLUSTER_NAME) 2>/dev/null || true
	@echo ""

	@echo "Step 2/10 — Install NGINX Ingress Controller..."
	@kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/kind/deploy.yaml
	@kubectl wait --namespace ingress-nginx --for=condition=ready pod --selector=app.kubernetes.io/component=controller --timeout=120s
	@echo "  Ingress controller ready"
	@echo ""

	@echo "Step 3/10 — Apply CRDs..."
	@kubectl apply -f $(K8S_E2E_HELM_DIR)/crds/dsb.io_sandboxes.yaml
	@kubectl get crd sandboxes.dsb.io
	@echo "  CRDs installed"
	@echo ""

	@echo "Step 4/10 — Build DSB server image..."
	@echo "  Building dsb/server:$(TAG) with kubernetes feature (this may take several minutes)..."
	@TAG=$(TAG) FEATURES=kubernetes docker compose -f $(DOCKER_DIR)/docker-compose.yml build $(K8S_E2E_DOCKER_BUILD_FLAGS) dsb-server 2>&1 | tail -20 || echo "  Build completed (check for errors)"
	@echo "  Loading image into kind cluster..."
	@kind load docker-image dsb/server:$(TAG) --name $(K8S_E2E_CLUSTER_NAME) 2>/dev/null || \
		echo "  Warning: Failed to load dsb/server. Image may not exist."
	@echo "  Loading sandbox images for agent-tester / create_sandbox (best effort)..."
	@for img in $(K8S_AGENT_SANDBOX_IMAGE) ubuntu:22.04 python:3.12; do \
		docker pull $$img 2>/dev/null || true; \
		kind load docker-image $$img --name $(K8S_E2E_CLUSTER_NAME) 2>/dev/null || echo "  skip $$img"; \
	done
	@echo "  Image loaded"
	@echo ""

	@echo "Step 5/10 — Ensure release namespace exists..."
	@kubectl create namespace $(K8S_E2E_NAMESPACE) --dry-run=client -o yaml | kubectl apply -f - 2>/dev/null || true
	@echo "  (Sandbox namespace $(K8S_E2E_SANDBOX_NAMESPACE) is created by Helm when backend=kubernetes — do not pre-create it.)"
	@echo ""

	@echo "Step 6/10 — Deploy PostgreSQL (required by DSB Helm chart)..."
	@POSTGRES_PASS=$$(openssl rand -hex 32); \
	echo "$$POSTGRES_PASS" > $(K8S_E2E_PG_PASS_FILE); \
	sed "s/__POSTGRES_PASSWORD_HEX__/$$POSTGRES_PASS/g" $(K8S_E2E_POSTGRES_YAML) | kubectl apply -n $(K8S_E2E_NAMESPACE) -f -; \
	kubectl rollout status deployment/postgres -n $(K8S_E2E_NAMESPACE) --timeout=120s
	@echo "  PostgreSQL ready"
	@echo ""

	@echo "Step 7/10 — Install DSB via Helm..."
	@API_KEY=$$(openssl rand -base64 32); \
	ADMIN_API_KEY=$$(openssl rand -base64 32); \
	VNC_API_KEY=$$(openssl rand -base64 32); \
	WEB_TERMINAL_API_KEY=$$(openssl rand -base64 32); \
	SSH_GATEWAY_API_KEY=$$(openssl rand -base64 32); \
	POSTGRES_PASS=$$(cat $(K8S_E2E_PG_PASS_FILE)); \
	helm upgrade --install dsb $(K8S_E2E_HELM_DIR) \
	  --namespace $(K8S_E2E_NAMESPACE) \
	  --create-namespace \
	  --set image.repository=dsb/server \
	  --set image.tag=$(TAG) \
	  --set postgres.password="$$POSTGRES_PASS" \
	  --set ingress.enabled=true \
	  --set ingress.className=nginx \
	  --set ingress.host=dsb.local \
	  --set config.sandbox.backend=kubernetes \
	  --set kubernetes.imagePrepull.enabled=false \
	  --set kubernetes.podReadyTimeoutSecs=$(K8S_E2E_POD_READY_TIMEOUT_SECS) \
	  --set kubernetes.resources.requests.cpu=$(K8S_E2E_SANDBOX_CPU_REQUEST) \
	  --set kubernetes.resources.requests.memory=$(K8S_E2E_SANDBOX_MEMORY_REQUEST) \
	  --set kubernetes.namespace=$(K8S_E2E_SANDBOX_NAMESPACE) \
	  --set secrets.apiKey="$$API_KEY" \
	  --set secrets.adminApiKey="$$ADMIN_API_KEY" \
	  --set secrets.vncApiKey="$$VNC_API_KEY" \
	  --set secrets.webTerminalApiKey="$$WEB_TERMINAL_API_KEY" \
	  --set secrets.sshGatewayApiKey="$$SSH_GATEWAY_API_KEY" \
	  --wait --timeout=300s 2>&1 || echo "  Helm install had warnings (continuing)..."
	@echo ""

	@echo "Step 8/10 — Verify Helm install..."
	@kubectl get pods -n $(K8S_E2E_NAMESPACE) 2>/dev/null || echo "  No pods yet"
	@kubectl get pods -n $(K8S_E2E_SANDBOX_NAMESPACE) 2>/dev/null || echo "  No sandbox pods yet"
	@kubectl get ingress -n $(K8S_E2E_NAMESPACE) 2>/dev/null || echo "  No ingress yet"
	@kubectl describe pod -n $(K8S_E2E_NAMESPACE) -l app.kubernetes.io/name=dsb 2>/dev/null | tail -20 || true
	@echo ""

	@echo "Step 9/10 — Run kubectl validation..."
	@echo "  Cluster nodes:"
	@kubectl get nodes 2>/dev/null || echo "  No nodes"
	@echo "  All pods:"
	@kubectl get pods -A 2>/dev/null || echo "  No pods"
	@echo ""

	@echo "Step 10/10 — Summary..."
	@echo "To port-forward and test:"
	@echo "  kubectl port-forward -n $(K8S_E2E_NAMESPACE) svc/dsb 8080:8080"
	@echo "Or run MCP agent-tester against this cluster:"
	@echo "  make test-agent-k8s"
	@echo ""
	@echo "To check server logs:"
	@echo "  kubectl logs -n $(K8S_E2E_NAMESPACE) -l app.kubernetes.io/name=dsb --tail=50 -f"
	@echo ""
	@echo "========================================"
	@echo "K8s E2E environment ready!"
	@echo "========================================"

# Rebuild dsb/sandbox, load into kind + deploy server (test-k8s-e2e), then run dsb-agent-tester.
# Prerequisite: base images for the server build (e.g. make base-images-build) if dsb/server has not been built yet.
test-k8s-agent-e2e: _check-config
	@echo "========================================"
	@echo "Full kind E2E: sandbox → cluster → agent tester"
	@echo "========================================"
	@$(MAKE) sandbox TAG=$(TAG)
	@$(MAKE) test-k8s-e2e K8S_E2E_DOCKER_BUILD_FLAGS=
	@$(MAKE) test-agent-k8s TAG=$(TAG)

test-k8s-e2e-cleanup:
	@echo "Tearing down K8s E2E environment..."
	@helm uninstall dsb -n $(K8S_E2E_NAMESPACE) 2>/dev/null || true
	@kubectl delete namespace $(K8S_E2E_NAMESPACE) 2>/dev/null || true
	@kubectl delete namespace $(K8S_E2E_SANDBOX_NAMESPACE) 2>/dev/null || true
	@echo "  Namespaces deleted"

test-k8s-e2e-delete-cluster:
	@echo "Deleting kind cluster..."
	@kind delete cluster --name $(K8S_E2E_CLUSTER_NAME) 2>/dev/null || true
	@echo "  Cluster deleted"
