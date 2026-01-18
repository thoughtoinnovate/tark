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
	@echo "E2E Tests (asciinema + agg):"
	@echo "  make e2e           - Run E2E visual tests (P1 core)"
	@echo "  make e2e-smoke     - Run P0 smoke tests (fast)"
	@echo "  make e2e-core      - Run P0+P1 core tests"
	@echo "  make help-e2e      - Show all E2E test commands"
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

# E2E Visual Tests - Uses asciinema + agg + expect
# No VHS/Chromium required - works in headless environments
.PHONY: build-test e2e e2e-deps e2e-chat e2e-tui e2e-all e2e-smoke e2e-core e2e-extended e2e-verify e2e-update e2e-clean e2e-list help-e2e

# Install E2E test dependencies (asciinema, agg, expect, ImageMagick)
e2e-deps:
	@echo "Installing E2E dependencies..."
	./tests/visual/e2e_runner.sh --install-deps

build-test:
	@echo "Building with test-sim feature..."
	cargo build --release --features test-sim

# One-stop command: build + run core tests
e2e: build-test
	./tests/visual/e2e_runner.sh --tier p1

# Run chat E2E tests (all tiers)
e2e-chat: build-test
	./tests/visual/e2e_runner.sh --tier all

# Run TUI E2E tests (future)
e2e-tui:
	@echo "TUI E2E tests not yet implemented"
	@echo "BDD features ready: tests/visual/tui/features/"

# Run all E2E tests (chat + tui)
e2e-all: e2e-chat e2e-tui

# P0 smoke tests (fast - for every commit)
e2e-smoke: build-test
	./tests/visual/e2e_runner.sh --tier p0

# P0+P1 core tests (for PRs)
e2e-core: build-test
	./tests/visual/e2e_runner.sh --tier p1

# P0+P1+P2 extended tests
e2e-extended: build-test
	./tests/visual/e2e_runner.sh --tier p2

# Compare current snapshots against baseline
e2e-verify:
	./tests/visual/e2e_runner.sh --verify

# Update baseline snapshots from current
e2e-update:
	./tests/visual/e2e_runner.sh --update-baseline

# Clean generated files
e2e-clean:
	./tests/visual/e2e_runner.sh --clean

# List available scenarios
e2e-list:
	./tests/visual/e2e_runner.sh --list

help-e2e:
	@echo "E2E Visual Tests (asciinema + agg)"
	@echo "==================================="
	@echo ""
	@echo "Quick Start:"
	@echo "  make e2e-deps         - Install asciinema, agg, expect, ImageMagick"
	@echo "  make e2e-smoke        - Run quick smoke test (P0)"
	@echo "  make e2e              - Run core tests (P1)"
	@echo ""
	@echo "Test Tiers:"
	@echo "  make e2e-smoke        - P0 smoke tests (fast, every commit)"
	@echo "  make e2e-core         - P0+P1 core tests (for PRs)"
	@echo "  make e2e-extended     - P0+P1+P2 extended tests"
	@echo ""
	@echo "Commands:"
	@echo "  make e2e-chat         - Run all chat E2E tests"
	@echo "  make e2e-tui          - Run TUI E2E tests (future)"
	@echo "  make e2e-all          - Run all E2E tests"
	@echo "  make e2e-list         - List available scenarios"
	@echo ""
	@echo "Verification:"
	@echo "  make e2e-verify       - Compare snapshots vs baseline"
	@echo "  make e2e-update       - Update baseline images"
	@echo "  make e2e-clean        - Remove generated files"
	@echo ""
	@echo "Outputs:"
	@echo "  tests/visual/chat/recordings/*.gif  - Animated GIF recordings"
	@echo "  tests/visual/chat/current/*.png     - Current run snapshots"
	@echo "  tests/visual/chat/snapshots/        - Baseline images"

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

