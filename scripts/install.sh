#!/usr/bin/env bash
#
# Deacon installer script
#
# Usage:
#   curl -fsSL https://get2knowio.github.io/deacon/install.sh | bash
#
# Environment variables:
#   DEACON_VERSION      - Version to install (e.g., "0.1.4" or "v0.1.4"), defaults to latest
#   DEACON_INSTALL_DIR  - Installation directory, defaults to ~/.local/bin or /usr/local/bin
#   DEACON_FORCE        - Set to "true" to overwrite existing installation without prompt
#   DEACON_NO_MODIFY_PATH - Set to "true" to skip PATH modification suggestions
#
# Supported platforms:
#   - Linux (x86_64, aarch64) - glibc and musl
#   - macOS (x86_64, aarch64/Apple Silicon)
#   - Windows (x86_64, aarch64) via Git Bash/MSYS2/Cygwin
#

set -euo pipefail

# Colors for output (disabled if not a terminal)
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    BOLD='\033[1m'
    NC='\033[0m' # No Color
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    BOLD=''
    NC=''
fi

GITHUB_REPO="get2knowio/deacon"
BINARY_NAME="deacon"

info() {
    echo -e "${BLUE}info:${NC} $*"
}

warn() {
    echo -e "${YELLOW}warn:${NC} $*" >&2
}

error() {
    echo -e "${RED}error:${NC} $*" >&2
}

success() {
    echo -e "${GREEN}success:${NC} $*"
}

# Check if a command exists
has_cmd() {
    command -v "$1" >/dev/null 2>&1
}

# Detect the operating system
detect_os() {
    local os
    os="$(uname -s)"
    case "$os" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *)
            error "Unsupported operating system: $os"
            exit 1
            ;;
    esac
}

# Detect the CPU architecture
detect_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)
            error "Unsupported architecture: $arch"
            exit 1
            ;;
    esac
}

# Detect if running on musl libc (Linux only)
detect_libc() {
    if [[ "$(detect_os)" != "linux" ]]; then
        echo "gnu"
        return
    fi

    # Check if ldd exists and inspect its output
    if has_cmd ldd; then
        if ldd --version 2>&1 | grep -qi musl; then
            echo "musl"
            return
        fi
    fi

    # Check for musl-based systems via /lib
    if [[ -f /lib/ld-musl-x86_64.so.1 ]] || [[ -f /lib/ld-musl-aarch64.so.1 ]]; then
        echo "musl"
        return
    fi

    # Default to glibc
    echo "gnu"
}

# Map OS/arch/libc to release target triple
get_target() {
    local os="$1"
    local arch="$2"
    local libc="$3"

    case "$os" in
        linux)
            echo "${arch}-unknown-linux-${libc}"
            ;;
        macos)
            echo "${arch}-apple-darwin"
            ;;
        windows)
            echo "${arch}-pc-windows-msvc"
            ;;
    esac
}

# Get the archive extension for the platform
get_archive_ext() {
    local os="$1"
    case "$os" in
        windows) echo "zip" ;;
        *)       echo "tar.gz" ;;
    esac
}

# Get the latest release version from GitHub
get_latest_version() {
    local url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    local version

    if has_cmd curl; then
        version=$(curl -fsSL "$url" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    elif has_cmd wget; then
        version=$(wget -qO- "$url" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    else
        error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi

    if [[ -z "$version" ]]; then
        error "Failed to fetch latest version from GitHub"
        exit 1
    fi

    echo "$version"
}

# Download a file
download() {
    local url="$1"
    local output="$2"

    info "Downloading: $url"

    if has_cmd curl; then
        curl -fsSL "$url" -o "$output"
    elif has_cmd wget; then
        wget -q "$url" -O "$output"
    else
        error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi
}

# Verify SHA256 checksum
verify_checksum() {
    local file="$1"
    local expected="$2"
    local actual

    if has_cmd sha256sum; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif has_cmd shasum; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    else
        warn "Neither sha256sum nor shasum found. Skipping checksum verification."
        return 0
    fi

    if [[ "$actual" != "$expected" ]]; then
        error "Checksum verification failed!"
        error "Expected: $expected"
        error "Actual:   $actual"
        return 1
    fi

    info "Checksum verified successfully"
    return 0
}

# Extract the archive
extract_archive() {
    local archive="$1"
    local dest="$2"
    local os="$3"

    case "$os" in
        windows)
            if has_cmd unzip; then
                unzip -q "$archive" -d "$dest"
            elif has_cmd 7z; then
                7z x -o"$dest" "$archive" >/dev/null
            else
                error "Neither unzip nor 7z found. Please install one of them."
                exit 1
            fi
            ;;
        *)
            tar -xzf "$archive" -C "$dest"
            ;;
    esac
}

# Get default installation directory
get_default_install_dir() {
    # Prefer ~/.local/bin if it exists or can be created
    local local_bin="$HOME/.local/bin"

    if [[ -d "$local_bin" ]] || [[ -w "$(dirname "$local_bin")" ]]; then
        echo "$local_bin"
    elif [[ -w "/usr/local/bin" ]]; then
        echo "/usr/local/bin"
    else
        echo "$local_bin"
    fi
}

# Check if directory is in PATH
is_in_path() {
    local dir="$1"
    [[ ":$PATH:" == *":$dir:"* ]]
}

# Main installation logic
main() {
    echo -e "${BOLD}Deacon Installer${NC}"
    echo ""

    # Detect platform
    local os arch libc target ext
    os=$(detect_os)
    arch=$(detect_arch)
    libc=$(detect_libc)
    target=$(get_target "$os" "$arch" "$libc")
    ext=$(get_archive_ext "$os")

    info "Detected platform: $os ($arch, $libc)"
    info "Release target: $target"

    # Determine version to install
    local version="${DEACON_VERSION:-}"
    if [[ -z "$version" ]]; then
        info "Fetching latest version..."
        version=$(get_latest_version)
    fi

    # Normalize version (ensure it starts with 'v')
    if [[ ! "$version" =~ ^v ]]; then
        version="v$version"
    fi

    info "Installing version: $version"

    # Determine installation directory
    local install_dir="${DEACON_INSTALL_DIR:-$(get_default_install_dir)}"
    local binary_path="$install_dir/$BINARY_NAME"

    # Add .exe extension on Windows
    if [[ "$os" == "windows" ]]; then
        binary_path="${binary_path}.exe"
    fi

    info "Installation directory: $install_dir"

    # Check for existing installation
    if [[ -f "$binary_path" ]]; then
        local existing_version
        existing_version=$("$binary_path" --version 2>/dev/null | head -n1 || echo "unknown")

        if [[ "${DEACON_FORCE:-false}" != "true" ]]; then
            warn "Deacon is already installed: $existing_version"
            echo -n "Do you want to overwrite? [y/N] "
            read -r response
            if [[ ! "$response" =~ ^[Yy]$ ]]; then
                info "Installation cancelled"
                exit 0
            fi
        else
            info "Overwriting existing installation (DEACON_FORCE=true)"
        fi
    fi

    # Create temporary directory
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Construct download URLs
    local archive_name="deacon-${version}-${target}.${ext}"
    local base_url="https://github.com/${GITHUB_REPO}/releases/download/${version}"
    local archive_url="${base_url}/${archive_name}"
    local checksum_url="${base_url}/SHA256SUMS"

    # Download archive
    local archive_path="$tmp_dir/$archive_name"
    download "$archive_url" "$archive_path"

    # Download and verify checksum
    local checksum_path="$tmp_dir/SHA256SUMS"
    download "$checksum_url" "$checksum_path"

    # Extract expected checksum for our archive
    local expected_checksum
    expected_checksum=$(grep "$archive_name" "$checksum_path" | awk '{print $1}')

    if [[ -z "$expected_checksum" ]]; then
        error "Could not find checksum for $archive_name in SHA256SUMS"
        exit 1
    fi

    verify_checksum "$archive_path" "$expected_checksum"

    # Extract archive
    info "Extracting archive..."
    local extract_dir="$tmp_dir/extract"
    mkdir -p "$extract_dir"
    extract_archive "$archive_path" "$extract_dir" "$os"

    # Create installation directory if it doesn't exist
    if [[ ! -d "$install_dir" ]]; then
        info "Creating directory: $install_dir"
        mkdir -p "$install_dir"
    fi

    # Install binary
    local src_binary="$extract_dir/$BINARY_NAME"
    if [[ "$os" == "windows" ]]; then
        src_binary="${src_binary}.exe"
    fi

    if [[ ! -f "$src_binary" ]]; then
        error "Binary not found in archive: $src_binary"
        exit 1
    fi

    info "Installing binary to: $binary_path"
    cp "$src_binary" "$binary_path"
    chmod +x "$binary_path"

    echo ""
    success "Deacon $version has been installed successfully!"
    echo ""

    # Verify installation
    if "$binary_path" --version >/dev/null 2>&1; then
        info "Installed version: $("$binary_path" --version)"
    fi

    # PATH guidance
    if [[ "${DEACON_NO_MODIFY_PATH:-false}" != "true" ]] && ! is_in_path "$install_dir"; then
        echo ""
        warn "$install_dir is not in your PATH"
        echo ""
        echo "Add it to your PATH by adding one of the following to your shell config:"
        echo ""
        echo "  # For bash (~/.bashrc or ~/.bash_profile):"
        echo "  export PATH=\"$install_dir:\$PATH\""
        echo ""
        echo "  # For zsh (~/.zshrc):"
        echo "  export PATH=\"$install_dir:\$PATH\""
        echo ""
        echo "  # For fish (~/.config/fish/config.fish):"
        echo "  set -gx PATH $install_dir \$PATH"
        echo ""
        echo "Then restart your shell or run: source ~/.bashrc (or equivalent)"
    fi

    echo ""
    info "Get started with: deacon --help"
}

main "$@"
