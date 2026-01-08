#!/bin/bash
# tark installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash
#
# Security: This script verifies SHA256 checksums to ensure binary integrity

set -e

VERSION="v0.4.3"
REPO="thoughtoinnovate/tark"
BINARY_NAME="tark"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
SKIP_VERIFY="${SKIP_VERIFY:-false}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

security() {
    echo -e "${CYAN}[SECURITY]${NC} $1"
}

# Detect OS and architecture
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)   os="linux" ;;
        Darwin*)  os="darwin" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *)        error "Unsupported operating system: $(uname -s)

Supported: Linux, macOS (Darwin), Windows
Note: FreeBSD/OpenBSD/NetBSD are not currently supported." ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="arm64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac

    # Note: Release binaries are statically linked (musl) so no libc suffix needed
    echo "${os}-${arch}"
}

# Get the download URL for the latest release
get_download_url() {
    local platform="$1"
    local asset_name="tark-${platform}"
    
    if [[ "$platform" == windows* ]]; then
        asset_name="${asset_name}.exe"
    fi

    if [ "$VERSION" = "latest" ]; then
        # Get latest release
        local release_url="https://api.github.com/repos/${REPO}/releases/latest"
        local download_url
        download_url=$(curl -sL "$release_url" | grep "browser_download_url.*${asset_name}\"" | head -1 | cut -d '"' -f 4)
        
        if [ -z "$download_url" ]; then
            error "Could not find binary for platform: ${platform}

Available platforms:
  - linux-x86_64       (Any Linux x64)
  - linux-arm64        (Any Linux ARM64)
  - darwin-x86_64      (macOS Intel)
  - darwin-arm64       (macOS Apple Silicon)
  - windows-x86_64     (Windows x64)
  - windows-arm64      (Windows ARM64)

Check releases: https://github.com/${REPO}/releases"
        fi
        
        echo "$download_url"
    else
        # Use specific version
        echo "https://github.com/${REPO}/releases/download/${VERSION}/${asset_name}"
    fi
}

# Get the checksum URL for a binary
get_checksum_url() {
    local binary_url="$1"
    echo "${binary_url}.sha256"
}

# Verify SHA256 checksum
verify_checksum() {
    local file="$1"
    local expected_checksum="$2"
    local actual_checksum
    
    if command -v sha256sum &> /dev/null; then
        actual_checksum=$(sha256sum "$file" | cut -d ' ' -f 1)
    elif command -v shasum &> /dev/null; then
        actual_checksum=$(shasum -a 256 "$file" | cut -d ' ' -f 1)
    else
        warn "Neither sha256sum nor shasum found. Skipping verification."
        return 0
    fi
    
    if [ "$actual_checksum" = "$expected_checksum" ]; then
        return 0
    else
        return 1
    fi
}

# Download checksum file and extract the hash
download_checksum() {
    local checksum_url="$1"
    local tmp_dir="$2"
    local checksum
    
    if curl -fsSL "$checksum_url" -o "${tmp_dir}/checksum.sha256" 2>/dev/null; then
        # Extract just the hash (first field)
        checksum=$(cut -d ' ' -f 1 "${tmp_dir}/checksum.sha256")
        echo "$checksum"
    else
        echo ""
    fi
}

# Download and install
install() {
    local platform download_url checksum_url tmp_dir expected_checksum

    info "Detecting platform..."
    platform=$(detect_platform)
    info "Platform: ${platform}"

    info "Fetching download URL..."
    download_url=$(get_download_url "$platform")
    info "Download URL: ${download_url}"

    # Create temp directory
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Download checksum first (for verification)
    if [ "$SKIP_VERIFY" != "true" ]; then
        security "Downloading checksum for verification..."
        checksum_url=$(get_checksum_url "$download_url")
        expected_checksum=$(download_checksum "$checksum_url" "$tmp_dir")
        
        if [ -n "$expected_checksum" ]; then
            security "Expected SHA256: ${expected_checksum}"
        else
            warn "Could not download checksum file. Binary verification will be skipped."
            warn "To force install without verification, use: SKIP_VERIFY=true ./install.sh"
        fi
    else
        warn "Checksum verification skipped (SKIP_VERIFY=true)"
    fi

    info "Downloading ${BINARY_NAME}..."
    if command -v curl &> /dev/null; then
        curl -fsSL "$download_url" -o "${tmp_dir}/${BINARY_NAME}"
    elif command -v wget &> /dev/null; then
        wget -q "$download_url" -O "${tmp_dir}/${BINARY_NAME}"
    else
        error "Neither curl nor wget found. Please install one of them."
    fi

    # Verify checksum
    if [ "$SKIP_VERIFY" != "true" ] && [ -n "$expected_checksum" ]; then
        security "Verifying binary integrity..."
        if verify_checksum "${tmp_dir}/${BINARY_NAME}" "$expected_checksum"; then
            success "Checksum verified! Binary is authentic."
        else
            error "SECURITY ALERT: Checksum verification FAILED!
            
The downloaded binary does not match the expected checksum.
This could indicate:
  - A corrupted download
  - A tampered binary (man-in-the-middle attack)
  - A version mismatch

Expected: ${expected_checksum}

For security, the installation has been aborted.
If you trust the source, you can bypass with: SKIP_VERIFY=true ./install.sh"
        fi
    fi

    # Make executable
    chmod +x "${tmp_dir}/${BINARY_NAME}"

    # Install
    info "Installing to ${INSTALL_DIR}..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "${tmp_dir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    else
        warn "Need sudo to install to ${INSTALL_DIR}"
        sudo mv "${tmp_dir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    fi

    # Verify installation
    if command -v "$BINARY_NAME" &> /dev/null; then
        success "tark installed successfully!"
        info "Version: $($BINARY_NAME --version)"
        if [ "$SKIP_VERIFY" != "true" ] && [ -n "$expected_checksum" ]; then
            security "Installation verified with SHA256 checksum"
        fi
        echo ""
        info "Next steps:"
        echo "  1. Set your API key:"
        echo "     export OPENAI_API_KEY='sk-...'"
        echo "     # or"
        echo "     export ANTHROPIC_API_KEY='sk-ant-...'"
        echo ""
        echo "  2. Add the Neovim plugin to your LazyVim config:"
        echo "     -- lua/plugins/tark.lua"
        echo "     return {"
        echo "       \"thoughtoinnovate/tark\","
        echo "       lazy = false,"
        echo "       opts = { server = { auto_start = true } },"
        echo "     }"
        echo ""
        echo "  3. Restart Neovim and the server will start automatically!"
    else
        warn "Installation completed, but ${BINARY_NAME} not found in PATH"
        info "You may need to add ${INSTALL_DIR} to your PATH"
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version|-v)
            VERSION="v0.4.3"
            shift 2
            ;;
        --install-dir|-d)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --skip-verify)
            SKIP_VERIFY="true"
            shift
            ;;
        --help|-h)
            echo "tark installer"
            echo ""
            echo "Usage: install.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -v, --version VERSION   Install specific version (default: latest)"
            echo "  -d, --install-dir DIR   Installation directory (default: /usr/local/bin)"
            echo "  --skip-verify           Skip SHA256 checksum verification (not recommended)"
            echo "  -h, --help              Show this help"
            echo ""
            echo "Security:"
            echo "  This installer verifies SHA256 checksums to ensure binary integrity."
            echo "  If verification fails, installation is aborted for your protection."
            echo ""
            echo "Examples:"
            echo "  curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash"
            echo "  ./install.sh --version v0.1.0"
            echo "  ./install.sh --install-dir ~/.local/bin"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Run installation
install

