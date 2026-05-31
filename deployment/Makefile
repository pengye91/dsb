# DSB Deployment Makefile
# Simple commands for managing DSB production deployment

.PHONY: setup setup-china start stop restart logs pull status config modify-tags help

# Default target
.DEFAULT_GOAL := help

# =============================================================================
# Help
# =============================================================================

help:
	@echo "DSB Deployment Commands"
	@echo ""
	@echo "Setup:"
	@echo "  make setup         - Copy example configs (.env, dsb.yaml)"
	@echo "  make setup-china   - Copy China-optimized configs"
	@echo ""
	@echo "Management:"
	@echo "  make start         - Start all services"
	@echo "  make stop          - Stop all services"
	@echo "  make restart       - Restart all services"
	@echo "  make logs          - View service logs"
	@echo "  make pull          - Update/pull latest images"
	@echo "  make status        - Show service status"
	@echo ""
	@echo "Configuration:"
	@echo "  make config        - Validate configuration"
	@echo "  make modify-tags   - Update image tags and registry"
	@echo "  make help          - Show this help"

# =============================================================================
# Setup
# =============================================================================

setup:
	@if [ ! -f .env ]; then \
		cp .env.example .env; \
		echo "✅ Created .env from .env.example"; \
		echo "   Review and edit: nano .env"; \
	else \
		echo "⚠️  .env already exists"; \
	fi
	@if [ ! -f dsb.yaml ]; then \
		cp dsb.yaml.example dsb.yaml; \
		echo "✅ Created dsb.yaml from dsb.yaml.example"; \
		echo ""; \
		echo "⚠️  IMPORTANT: Edit dsb.yaml and set:"; \
		echo "   - server.api_key"; \
		echo "   - server.admin_api_key"; \
		echo "   - database.password"; \
		echo ""; \
		echo "   Generate keys with: openssl rand -hex 32"; \
	else \
		echo "⚠️  dsb.yaml already exists"; \
	fi

setup-china:
	@if [ ! -f .env ]; then \
		cp .env.china.example .env; \
		echo "✅ Created .env from .env.china.example"; \
	else \
		echo "⚠️  .env already exists"; \
	fi
	@if [ ! -f dsb.yaml ]; then \
		cp dsb.yaml.china.example dsb.yaml; \
		echo "✅ Created dsb.yaml from dsb.yaml.china.example"; \
		echo ""; \
		echo "⚠️  IMPORTANT: Edit dsb.yaml and set API keys and database password"; \
	else \
		echo "⚠️  dsb.yaml already exists"; \
	fi

# =============================================================================
# Management
# =============================================================================

start: config
	@echo "🚀 Starting DSB services..."
	env -u NO_PROXY -u no_proxy -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY docker compose up -d
	@echo ""
	@echo "⏳ Waiting for services to be healthy..."
	@sleep 5
	@docker compose ps
	@echo ""
	@echo "✅ Services started!"
	@echo "   Dashboard: http://localhost:$$(grep DSB_DASHBOARD_HOST_PORT .env | cut -d= -f2 || echo 3001)"
	@echo "   API:       http://localhost:$$(grep DSB_SERVER_HOST_PORT .env | cut -d= -f2 || echo 8080)"

stop:
	@echo "🛑 Stopping DSB services..."
	docker compose down
	@echo "✅ Services stopped"

restart:
	@echo "🔄 Restarting DSB services..."
	env -u NO_PROXY -u no_proxy -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY docker compose restart
	@echo "✅ Services restarted"

logs:
	@docker compose logs --tail=100 -f

pull:
	@echo "📥 Pulling latest images..."
	docker compose pull
	@echo "✅ Images updated"

status:
	@docker compose ps

# =============================================================================
# Tag Management
# =============================================================================

modify-tags:
	@if [ -z "$(SERVICES_REGISTRY)" ] && [ -z "$(SERVICES_TAG)" ] && [ -z "$(DEFAULT_SANDBOX_TAG)" ]; then \
		echo "❌ No tags specified"; \
		echo ""; \
		echo "Usage:"; \
		echo "  make modify-tags SERVICES_REGISTRY=<registry> SERVICES_TAG=<tag> DEFAULT_SANDBOX_TAG=<tag>"; \
		echo ""; \
		echo "All parameters are optional — only specified values will be updated."; \
		echo ""; \
		echo "Example:"; \
		echo "  make modify-tags SERVICES_REGISTRY=docker.io/ SERVICES_TAG=dev-v0.10.6 DEFAULT_SANDBOX_TAG=dev-v0.10.6"; \
		exit 1; \
	fi
	@if [ -n "$(SERVICES_REGISTRY)" ]; then \
		sed -i 's|^DOCKER_REGISTRY=.*|DOCKER_REGISTRY=$(SERVICES_REGISTRY)|' .env; \
		sed -i 's|^  registry:.*|  registry: "$(SERVICES_REGISTRY)"|' dsb.yaml; \
		sed -i 's|^\(  default_image: "\)[^/]*|\1$(SERVICES_REGISTRY)|' dsb.yaml; \
		echo "✅ Updated registry to $(SERVICES_REGISTRY) (.env + dsb.yaml)"; \
	fi
	@if [ -n "$(SERVICES_TAG)" ]; then \
		sed -i 's|^IMAGE_TAG=.*|IMAGE_TAG=$(SERVICES_TAG)|' .env; \
		echo "✅ Updated services tag to $(SERVICES_TAG) (.env)"; \
	fi
	@if [ -n "$(DEFAULT_SANDBOX_TAG)" ]; then \
		sed -i 's|^\(  default_image:.*:\)[^"]*|\1$(DEFAULT_SANDBOX_TAG)|' dsb.yaml; \
		echo "✅ Updated sandbox tag to $(DEFAULT_SANDBOX_TAG) (dsb.yaml)"; \
	fi

# =============================================================================
# Configuration
# =============================================================================

config:
	@echo "🔍 Validating configuration..."
	@if [ ! -f .env ]; then \
		echo "❌ Missing .env file. Run 'make setup' first."; \
		exit 1; \
	fi
	@if [ ! -f dsb.yaml ]; then \
		echo "❌ Missing dsb.yaml file. Run 'make setup' first."; \
		exit 1; \
	fi
	@echo "✅ Configuration files exist"
	@echo "📋 Validating docker-compose.yml..."
	@docker compose config > /dev/null && echo "✅ Docker Compose config valid"