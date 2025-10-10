#!/usr/bin/env bash

# Deacon Installation Script
# 
# This script downloads and installs the latest release of deacon for your platform.
# It automatically detects your operating system and architecture, downloads the
# appropriate binary, verifies its checksum, and installs it to a directory in your PATH.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/get2knowio/deacon/main/scripts/install.sh | bash
#   # or with a specific version (with or without leading 'v')
#   curl -fsSL https://raw.githubusercontent.com/get2knowio/deacon/main/scripts/install.sh | DEACON_VERSION=0.1.3 bash
#   
# Environment Variables:
#   DEACON_VERSION: Specific version to install (default: latest). Accepts 'v0.1.3' or '0.1.3'
#   DEACON_BASE_URL: Base URL for releases (default: GitHub releases)
#   DEACON_INSTALL_DIR: Installation directory (default: /usr/local/bin or ~/.local/bin)
#   DEACON_FORCE: Skip confirmation prompts (default: false)

set -euo pipefail

# Default configuration
GITHUB_REPO="get2knowio/deacon"
DEFAULT_BASE_URL="https://github.com/${GITHUB_REPO}/releases"
DEFAULT_INSTALL_DIR=""

# Override defaults with environment variables
VERSION="${DEACON_VERSION:-latest}"
BASE_URL="${DEACON_BASE_URL:-$DEFAULT_BASE_URL}"
INSTALL_DIR="${DEACON_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
FORCE="${DEACON_FORCE:-false}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1" >&2
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1" >&2
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" >&2
}

# Error handling
cleanup() {
    if [[ -n "${TEMP_DIR:-}" ]] && [[ -d "$TEMP_DIR" ]]; then
        rm -rf "$TEMP_DIR"
    fi
}

error_exit() {
    log_error "$1"
    cleanup
    exit 1
}

trap cleanup EXIT

# Platform detection
detect_platform() {
    local os arch libc
    
    # Detect OS
    case "$(uname -s)" in
        Linux*)     os="linux" ;;
        Darwin*)    os="darwin" ;;
        CYGWIN*|MINGW*|MSYS*)    os="windows" ;;
        *)          error_exit "Unsupported operating system: $(uname -s)" ;;
    esac
    
    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              error_exit "Unsupported architecture: $(uname -m)" ;;
    esac
    
    # Detect libc on Linux (gnu vs musl)
    libc=""
    if [[ "$os" == "linux" ]]; then
        if command -v ldd >/dev/null 2>&1; then
            if ldd --version 2>&1 | grep -qi musl; then
                libc="musl"
            else
                libc="gnu"
            fi
        elif [[ -f "/etc/alpine-release" ]]; then
            libc="musl"
        else
            libc="gnu"
        fi
    fi

    # Map to Rust target triple
    case "${os}-${arch}-${libc}" in
        linux-x86_64-musl)     echo "x86_64-unknown-linux-musl" ;;
        linux-aarch64-musl)    echo "aarch64-unknown-linux-musl" ;;
        linux-x86_64-gnu)      echo "x86_64-unknown-linux-gnu" ;;
        linux-aarch64-gnu)     echo "aarch64-unknown-linux-gnu" ;;
        darwin-x86_64-*)       echo "x86_64-apple-darwin" ;;
        darwin-aarch64-*)      echo "aarch64-apple-darwin" ;;
        windows-x86_64-*)      echo "x86_64-pc-windows-msvc" ;;
        *)                     error_exit "Unsupported platform: ${os}-${arch}" ;;
    esac
}

# Get the latest version from GitHub API
get_latest_version() {
    if ! command -v curl >/dev/null 2>&1; then
        error_exit "curl is required but not installed"
    fi
    
    local api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    local version
    
    log_info "Fetching latest version information..."
    
    if command -v jq >/dev/null 2>&1; then
        version=$(curl -fsSL "$api_url" | jq -r '.tag_name')
    else
        # Fallback parsing without jq
        version=$(curl -fsSL "$api_url" | grep '"tag_name":' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/')
    fi
    
    if [[ -z "$version" ]] || [[ "$version" == "null" ]]; then
        error_exit "Failed to get latest version from GitHub API"
    fi
    
    echo "$version"
}

# Determine installation directory
get_install_dir() {
    if [[ -n "$INSTALL_DIR" ]]; then
        echo "$INSTALL_DIR"
        return
    fi
    
    # Try system-wide installation first
    if [[ -w "/usr/local/bin" ]] || [[ "$EUID" -eq 0 ]]; then
        echo "/usr/local/bin"
    elif [[ -d "$HOME/.local/bin" ]] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
        echo "$HOME/.local/bin"
    else
        error_exit "Cannot determine a writable installation directory. Please set DEACON_INSTALL_DIR."
    fi
}

# Verify checksum
verify_checksum() {
    local file="$1"
    local expected_checksum="$2"
    
    log_info "Verifying checksum..."
    
    local actual_checksum
    if command -v sha256sum >/dev/null 2>&1; then
        actual_checksum=$(sha256sum "$file" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then
        actual_checksum=$(shasum -a 256 "$file" | cut -d' ' -f1)
    else
        log_warn "No SHA256 utility found. Skipping checksum verification."
        return 0
    fi
    
    if [[ "$actual_checksum" != "$expected_checksum" ]]; then
        error_exit "Checksum verification failed! Expected: $expected_checksum, Got: $actual_checksum"
    fi
    
    log_success "Checksum verification passed"
}

# Extract archive
extract_archive() {
    local archive="$1"
    local target_dir="$2"
    
    log_info "Extracting archive..."
    
    case "$archive" in
        *.tar.gz|*.tgz)
            if ! tar -xzf "$archive" -C "$target_dir"; then
                error_exit "Failed to extract tar.gz archive"
            fi
            ;;
        *.zip)
            if command -v unzip >/dev/null 2>&1; then
                if ! unzip -q "$archive" -d "$target_dir"; then
                    error_exit "Failed to extract zip archive"
                fi
            else
                error_exit "unzip is required to extract .zip files but not installed"
            fi
            ;;
        *)
            error_exit "Unsupported archive format: $archive"
            ;;
    esac
}

# Main installation function
install_deacon() {
    local platform version install_dir tag
    
    platform=$(detect_platform)
    log_info "Detected platform: $platform"
    
    if [[ "$VERSION" == "latest" ]]; then
        version=$(get_latest_version)
    else
        version="$VERSION"
    fi

    # Normalize version tag to include leading 'v' (supports inputs like '0.1.3' or 'v0.1.3')
    tag="$version"
    if [[ "$tag" != v* ]]; then
        tag="v$tag"
    fi
    log_info "Installing version: $tag"
    
    install_dir=$(get_install_dir)
    log_info "Installation directory: $install_dir"
    
    # Determine file extension and binary name
    local file_ext binary_name
    case "$platform" in
        *windows*)
            file_ext="zip"
            binary_name="deacon.exe"
            ;;
        *)
            file_ext="tar.gz"
            binary_name="deacon"
            ;;
    esac
    
    # Construct URLs and filenames
    local archive_name="deacon-${tag}-${platform}.${file_ext}"
    local checksums_name="SHA256SUMS"
    
    local download_url
    if [[ "$BASE_URL" == *"github.com"* ]] && [[ "$VERSION" == "latest" ]]; then
        download_url="${BASE_URL}/latest/download"
    else
        download_url="${BASE_URL}/download/${tag}"
    fi
    
    local archive_url="${download_url}/${archive_name}"
    local checksums_url="${download_url}/${checksums_name}"
    
    # Create temporary directory
    TEMP_DIR=$(mktemp -d)
    local archive_path="${TEMP_DIR}/${archive_name}"
    local checksums_path="${TEMP_DIR}/${checksums_name}"
    
    # Download archive
    log_info "Downloading $archive_name..."
    if ! curl -fsSL "$archive_url" -o "$archive_path"; then
        error_exit "Failed to download $archive_url"
    fi
    
    # Download and verify checksums
    log_info "Downloading checksums..."
    if curl -fsSL "$checksums_url" -o "$checksums_path" 2>/dev/null; then
        local expected_checksum
        expected_checksum=$(grep "$archive_name" "$checksums_path" | cut -d' ' -f1)
        if [[ -n "$expected_checksum" ]]; then
            verify_checksum "$archive_path" "$expected_checksum"
        else
            log_warn "Checksum for $archive_name not found in SHA256SUMS file"
        fi
    else
        log_warn "Failed to download checksums file. Skipping verification."
    fi
    
    # Extract archive
    extract_archive "$archive_path" "$TEMP_DIR"
    
    # Find the binary
    local binary_path
    if [[ -f "${TEMP_DIR}/${binary_name}" ]]; then
        binary_path="${TEMP_DIR}/${binary_name}"
    else
        # Look for binary in subdirectories
        binary_path=$(find "$TEMP_DIR" -name "$binary_name" -type f | head -1)
        if [[ -z "$binary_path" ]]; then
            error_exit "Binary $binary_name not found in extracted archive"
        fi
    fi
    
    # Create install directory if it doesn't exist
    if [[ ! -d "$install_dir" ]]; then
        if ! mkdir -p "$install_dir"; then
            error_exit "Failed to create installation directory: $install_dir"
        fi
    fi
    
    # Install binary
    local final_path="${install_dir}/${binary_name}"
    if [[ -f "$final_path" ]] && [[ "$FORCE" != "true" ]]; then
        read -p "deacon is already installed at $final_path. Overwrite? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Installation cancelled"
            exit 0
        fi
    fi
    
    log_info "Installing binary to $final_path..."
    if ! cp "$binary_path" "$final_path"; then
        error_exit "Failed to install binary to $final_path"
    fi
    
    if ! chmod +x "$final_path"; then
        error_exit "Failed to make binary executable"
    fi
    
    log_success "deacon ${version} installed successfully to ${final_path}"
    
    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":$install_dir:"* ]]; then
        log_warn "Installation directory $install_dir is not in your PATH"
        if [[ "$install_dir" == "$HOME/.local/bin" ]]; then
            log_info "Add the following line to your shell configuration (~/.bashrc, ~/.zshrc, etc.):"
            echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        fi
    fi
    
    # Test installation
    if command -v "$binary_name" >/dev/null 2>&1; then
        log_success "Installation verified: $(command -v "$binary_name")"
        log_info "Run '$binary_name --help' to get started"
    else
        log_warn "Binary not found in PATH. You may need to restart your shell or update your PATH"
    fi
}

# Script entry point
main() {
    log_info "Starting deacon installation..."
    log_info "Repository: $GITHUB_REPO"
    log_info "Version: $VERSION"
    log_info "Base URL: $BASE_URL"
    
    install_deacon
}

# Run main function
main "$@"