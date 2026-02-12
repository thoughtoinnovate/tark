#!/bin/bash
# tark installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash
#
# Security: This script verifies SHA256 checksums to ensure binary integrity

set -e

VERSION="v0.11.9"
PREVIOUS_VERSION="v0.11.8"
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

# Get the download URL for a specific version
get_download_url() {
    local version="$1"
    local platform="$2"
    local asset_name="tark-${platform}"
    
    if [[ "$platform" == windows* ]]; then
        asset_name="${asset_name}.exe"
    fi

    echo "https://github.com/${REPO}/releases/download/${version}/${asset_name}"
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

# Attempt to download a version
try_download() {
    local version="$1"
    local platform="$2"
    local tmp_dir="$3"
    
    info "Attempting to download version ${version}..."
    local download_url
    download_url=$(get_download_url "$version" "$platform")
    info "Download URL: ${download_url}"

    # Download checksum first
    local checksum_url expected_checksum
    if [ "$SKIP_VERIFY" != "true" ]; then
        security "Downloading checksum for verification..."
        checksum_url=$(get_checksum_url "$download_url")
        expected_checksum=$(download_checksum "$checksum_url" "$tmp_dir")
        
        if [ -n "$expected_checksum" ]; then
            security "Expected SHA256: ${expected_checksum}"
        else
            warn "Could not download checksum file for version ${version}. Binary verification will be skipped."
        fi
    else
        warn "Checksum verification skipped (SKIP_VERIFY=true)"
    fi

    info "Downloading ${BINARY_NAME}..."
    if command -v curl &> /dev/null; then
        if ! curl -fsSL "$download_url" -o "${tmp_dir}/${BINARY_NAME}"; then
            warn "Failed to download version ${version}."
            return 1
        fi
    elif command -v wget &> /dev/null; then
        if ! wget -q "$download_url" -O "${tmp_dir}/${BINARY_NAME}"; then
            warn "Failed to download version ${version}."
            return 1
        fi
    else
        error "Neither curl nor wget found. Please install one of them."
    fi

    # Verify checksum
    if [ "$SKIP_VERIFY" != "true" ] && [ -n "$expected_checksum" ]; then
        security "Verifying binary integrity..."
        if verify_checksum "${tmp_dir}/${BINARY_NAME}" "$expected_checksum"; then
            success "Checksum verified! Binary is authentic."
        else
            error "SECURITY ALERT: Checksum verification FAILED for version ${version}!"
            return 1
        fi
    fi
    
    success "Successfully downloaded version ${version}."
    echo "$version"
    return 0
}

# Download and install
install() {
    local platform tmp_dir installed_version

    info "Detecting platform..."
    platform=$(detect_platform)
    info "Platform: ${platform}"

    # Create temp directory
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Try to download the primary version, fallback to the previous one
    if installed_version_raw=$(try_download "$VERSION" "$platform" "$tmp_dir"); then
        installed_version=$(echo "$installed_version_raw" | tail -n 1)

        :
    elif installed_version=$(try_download "$PREVIOUS_VERSION" "$platform" "$tmp_dir"); then
        warn "Primary version ${VERSION} failed, but successfully downloaded fallback version ${PREVIOUS_VERSION}."
    else
        error "Failed to download both primary version ${VERSION} and fallback version ${PREVIOUS_VERSION}. Aborting."
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
        info "Installed Version: ${installed_version}"
        actual_version=$($BINARY_NAME --version)
        info "Reported Version: ${actual_version}"
        
        local installed_version_for_compare=$(echo "$installed_version" | sed 's/^v//')
        if [ "$installed_version_for_compare" != "$(echo "$actual_version" | awk '{print $2}')" ]; then
             warn "Installed version (${installed_version}) does not match reported version (${actual_version})."
        fi

        if [ "$SKIP_VERIFY" != "true" ]; then
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
            VERSION="$2"
            PREVIOUS_VERSION="" # No fallback when specific version is requested
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
            echo "  -v, --version VERSION   Install specific version (default: ${VERSION}, fallback: ${PREVIOUS_VERSION})"
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
            echo "  ./install.sh --version v0.11.9"
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
