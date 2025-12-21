# tark Makefile
# Run `make help` for available commands

.PHONY: help env env-rust env-docker build build-release test lint fmt clean docker-build docker-run docker-stop docker-logs install

# Default target
help:
	@echo "tark Development Commands"
	@echo "========================="
	@echo ""
	@echo "Setup:"
	@echo "  make env           - Set up complete dev environment (Rust + dependencies)"
	@echo "  make env-rust      - Install Rust toolchain only"
	@echo "  make env-docker    - Install Docker (shows instructions)"
	@echo ""
	@echo "Development:"
	@echo "  make build         - Build debug binary"
	@echo "  make build-release - Build release binary"
	@echo "  make test          - Run tests"
	@echo "  make lint          - Run clippy linter"
	@echo "  make fmt           - Format code"
	@echo "  make clean         - Clean build artifacts"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-build  - Build Docker image locally"
	@echo "  make docker-run    - Run tark in Docker container"
	@echo "  make docker-stop   - Stop Docker container"
	@echo "  make docker-logs   - Show Docker container logs"
	@echo "  make docker-test   - Full Docker build and test"
	@echo ""
	@echo "Install:"
	@echo "  make install       - Install tark binary to ~/.cargo/bin"
	@echo ""

# =============================================================================
# Environment Setup
# =============================================================================

env: env-check-os env-rust env-deps
	@echo ""
	@echo "✅ Development environment ready!"
	@echo ""
	@echo "Next steps:"
	@echo "  source ~/.cargo/env   # If Rust was just installed"
	@echo "  make build            # Build the project"
	@echo ""

env-check-os:
	@echo "Detecting OS..."
	@uname -a

env-rust:
	@echo "Setting up Rust..."
	@if command -v rustc >/dev/null 2>&1; then \
		echo "✓ Rust is already installed: $$(rustc --version)"; \
	else \
		echo "Installing Rust via rustup..."; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
		echo "✓ Rust installed. Run: source ~/.cargo/env"; \
	fi

env-deps:
	@echo "Checking build dependencies..."
	@if [ "$$(uname)" = "Darwin" ]; then \
		echo "macOS detected"; \
		if ! command -v brew >/dev/null 2>&1; then \
			echo "⚠ Homebrew not found. Install from https://brew.sh"; \
		else \
			echo "Installing dependencies via Homebrew..."; \
			brew install openssl pkg-config || true; \
		fi; \
	elif [ "$$(uname)" = "Linux" ]; then \
		echo "Linux detected"; \
		if command -v apt-get >/dev/null 2>&1; then \
			echo "Debian/Ubuntu detected"; \
			echo "Installing: build-essential pkg-config libssl-dev"; \
			sudo apt-get update && sudo apt-get install -y build-essential pkg-config libssl-dev || \
				echo "⚠ Could not install deps. Run manually: sudo apt-get install build-essential pkg-config libssl-dev"; \
		elif command -v dnf >/dev/null 2>&1; then \
			echo "Fedora/RHEL detected"; \
			sudo dnf install -y gcc gcc-c++ openssl-devel pkg-config || \
				echo "⚠ Could not install deps. Run manually: sudo dnf install gcc gcc-c++ openssl-devel pkg-config"; \
		elif command -v pacman >/dev/null 2>&1; then \
			echo "Arch Linux detected"; \
			sudo pacman -S --noconfirm base-devel openssl pkg-config || \
				echo "⚠ Could not install deps. Run manually: sudo pacman -S base-devel openssl pkg-config"; \
		elif command -v apk >/dev/null 2>&1; then \
			echo "Alpine Linux detected"; \
			sudo apk add --no-cache build-base openssl-dev pkgconfig || \
				echo "⚠ Could not install deps. Run manually: sudo apk add build-base openssl-dev pkgconfig"; \
		else \
			echo "⚠ Unknown Linux distro. Please install: gcc, g++, openssl-dev, pkg-config"; \
		fi; \
	else \
		echo "⚠ Unknown OS: $$(uname). Please install build tools manually."; \
	fi
	@echo "✓ Dependencies check complete"

env-docker:
	@echo "Docker Installation Instructions"
	@echo "================================="
	@echo ""
	@if command -v docker >/dev/null 2>&1; then \
		echo "✓ Docker is already installed: $$(docker --version)"; \
	else \
		echo "Docker is not installed. Install from:"; \
		echo ""; \
		echo "  macOS:   https://docs.docker.com/desktop/mac/install/"; \
		echo "  Linux:   https://docs.docker.com/engine/install/"; \
		echo "  Windows: https://docs.docker.com/desktop/windows/install/"; \
		echo ""; \
		echo "Or use the convenience script (Linux only):"; \
		echo "  curl -fsSL https://get.docker.com | sh"; \
	fi

# =============================================================================
# Development
# =============================================================================

build:
	@echo "Building debug binary..."
	cargo build

build-release:
	@echo "Building release binary..."
	cargo build --release
	@echo "Binary: target/release/tark"

test:
	@echo "Running tests..."
	cargo test

lint:
	@echo "Running clippy..."
	cargo clippy -- -D warnings

fmt:
	@echo "Formatting code..."
	cargo fmt

fmt-check:
	@echo "Checking formatting..."
	cargo fmt -- --check

clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf target/

# =============================================================================
# Docker
# =============================================================================

DOCKER_IMAGE ?= tark:local-alpine
DOCKER_CONTAINER ?= tark-server

docker-build:
	@echo "Building Docker image..."
	docker build -f Dockerfile.alpine -t $(DOCKER_IMAGE) .
	@echo ""
	@echo "Image size:"
	@docker images $(DOCKER_IMAGE) --format "{{.Repository}}:{{.Tag}} - {{.Size}}"

docker-build-minimal:
	@echo "Building minimal Docker image..."
	docker build -f Dockerfile -t tark:local .
	@echo ""
	@echo "Image size:"
	@docker images tark:local --format "{{.Repository}}:{{.Tag}} - {{.Size}}"

docker-run:
	@echo "Starting tark container..."
	@docker rm -f $(DOCKER_CONTAINER) 2>/dev/null || true
	docker run -d --name $(DOCKER_CONTAINER) \
		-p 8765:8765 \
		-v $$(pwd):/workspace \
		-e OPENAI_API_KEY="$${OPENAI_API_KEY:-}" \
		-e ANTHROPIC_API_KEY="$${ANTHROPIC_API_KEY:-}" \
		$(DOCKER_IMAGE)
	@echo ""
	@echo "Container started. Health check:"
	@sleep 2
	@curl -sf http://localhost:8765/health && echo "" || echo "Waiting for startup..."

docker-stop:
	@echo "Stopping tark container..."
	@docker stop $(DOCKER_CONTAINER) 2>/dev/null || true
	@docker rm $(DOCKER_CONTAINER) 2>/dev/null || true
	@echo "✓ Container stopped"

docker-logs:
	docker logs -f $(DOCKER_CONTAINER)

docker-shell:
	docker exec -it $(DOCKER_CONTAINER) sh

docker-test: docker-build
	@echo ""
	@echo "Running Docker test..."
	./test-docker-build.sh

# =============================================================================
# Install
# =============================================================================

install:
	@echo "Installing tark to ~/.cargo/bin..."
	cargo install --path .
	@echo ""
	@echo "✓ Installed! Run: tark --version"

install-release:
	@echo "Building and installing release binary..."
	cargo install --path . --release
	@echo ""
	@echo "✓ Installed! Run: tark --version"

# =============================================================================
# CI
# =============================================================================

ci: fmt-check lint test
	@echo "✓ All CI checks passed"

