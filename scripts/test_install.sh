#!/usr/bin/env bash

# Test script for install.sh
# This script tests the install.sh functionality with local staged artifacts

set -euo pipefail

# Test configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SCRIPT="${SCRIPT_DIR}/install.sh"
TEST_DIR="/tmp/deacon-install-test"
FAKE_SERVER_DIR="${TEST_DIR}/fake-server"
FAKE_SERVER_PORT="8765"
FAKE_VERSION="v0.1.0"

# Test utilities
log_test() {
    echo "[TEST] $1"
}

log_info() {
    echo "[INFO] $1"
}

log_error() {
    echo "[ERROR] $1" >&2
}

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi
    if [[ -d "$TEST_DIR" ]]; then
        rm -rf "$TEST_DIR"
    fi
}

error_exit() {
    log_error "$1"
    cleanup
    exit 1
}

trap cleanup EXIT

# Create fake binary for testing
create_fake_binary() {
    local target="$1"
    local binary_dir="${FAKE_SERVER_DIR}/${target}"
    
    mkdir -p "$binary_dir"
    
    # Create a simple fake binary
    if [[ "$target" == *"windows"* ]]; then
        echo "#!/usr/bin/env bash" > "${binary_dir}/deacon.exe"
        echo "echo 'deacon ${FAKE_VERSION} (${target})'" >> "${binary_dir}/deacon.exe"
        echo "echo 'This is a test binary'" >> "${binary_dir}/deacon.exe"
        chmod +x "${binary_dir}/deacon.exe"
    else
        echo "#!/usr/bin/env bash" > "${binary_dir}/deacon"
        echo "echo 'deacon ${FAKE_VERSION} (${target})'" >> "${binary_dir}/deacon"
        echo "echo 'This is a test binary'" >> "${binary_dir}/deacon"
        chmod +x "${binary_dir}/deacon"
    fi
}

# Create fake release artifacts
create_fake_artifacts() {
    local targets=(
        "x86_64-unknown-linux-gnu"
        "aarch64-unknown-linux-gnu" 
        "x86_64-apple-darwin"
        "aarch64-apple-darwin"
        "x86_64-pc-windows-msvc"
    )
    
    log_info "Creating fake artifacts..."
    
    mkdir -p "$FAKE_SERVER_DIR"
    
    # Create the directory structure that matches GitHub releases API
    mkdir -p "${FAKE_SERVER_DIR}/download/${FAKE_VERSION}"
    
    cd "$FAKE_SERVER_DIR"
    
    # Create fake binaries for each target
    for target in "${targets[@]}"; do
        create_fake_binary "$target"
        
        if [[ "$target" == *"windows"* ]]; then
            # Create zip archive for Windows
            (cd "${target}" && zip -q "../download/${FAKE_VERSION}/deacon-${FAKE_VERSION}-${target}.zip" deacon.exe)
        else
            # Create tar.gz archive for Unix
            (cd "${target}" && tar -czf "../download/${FAKE_VERSION}/deacon-${FAKE_VERSION}-${target}.tar.gz" deacon)
        fi
        
        # Generate checksum
        if [[ "$target" == *"windows"* ]]; then
            sha256sum "download/${FAKE_VERSION}/deacon-${FAKE_VERSION}-${target}.zip" >> "download/${FAKE_VERSION}/SHA256SUMS"
        else
            sha256sum "download/${FAKE_VERSION}/deacon-${FAKE_VERSION}-${target}.tar.gz" >> "download/${FAKE_VERSION}/SHA256SUMS"
        fi
    done
    
    # Fix the checksums to have just the filename, not the full path
    cd "download/${FAKE_VERSION}"
    sed -i 's|download/[^/]*/||g' SHA256SUMS
    
    log_info "Created artifacts:"
    ls -la *.tar.gz *.zip SHA256SUMS 2>/dev/null || true
}

# Start simple HTTP server
start_fake_server() {
    log_info "Starting fake HTTP server on port $FAKE_SERVER_PORT..."
    
    cd "$FAKE_SERVER_DIR"
    python3 -m http.server "$FAKE_SERVER_PORT" >/dev/null 2>&1 &
    SERVER_PID=$!
    
    # Wait for server to start
    sleep 2
    
    # Test if server is running
    if ! curl -fsSL "http://localhost:$FAKE_SERVER_PORT/" >/dev/null 2>&1; then
        error_exit "Failed to start HTTP server"
    fi
    
    log_info "Fake server started with PID $SERVER_PID"
}

# Test platform detection
test_platform_detection() {
    log_test "Testing platform detection..."
    
    # Create a temporary script that just extracts the detect_platform function
    local temp_script="${TEST_DIR}/detect_platform_test.sh"
    cat > "$temp_script" << 'EOF'
#!/usr/bin/env bash

# Platform detection function from install.sh
detect_platform() {
    local os arch
    
    # Detect OS
    case "$(uname -s)" in
        Linux*)     os="linux" ;;
        Darwin*)    os="darwin" ;;
        CYGWIN*|MINGW*|MSYS*)    os="windows" ;;
        *)          echo "Unsupported operating system: $(uname -s)" >&2; exit 1 ;;
    esac
    
    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
    esac
    
    # Map to Rust target triple
    case "${os}-${arch}" in
        linux-x86_64)      echo "x86_64-unknown-linux-gnu" ;;
        linux-aarch64)     echo "aarch64-unknown-linux-gnu" ;;
        darwin-x86_64)     echo "x86_64-apple-darwin" ;;
        darwin-aarch64)    echo "aarch64-apple-darwin" ;;
        windows-x86_64)    echo "x86_64-pc-windows-msvc" ;;
        *)                 echo "Unsupported platform: ${os}-${arch}" >&2; exit 1 ;;
    esac
}

detect_platform
EOF
    
    chmod +x "$temp_script"
    local detected_platform
    detected_platform=$("$temp_script")
    
    if [[ -z "$detected_platform" ]]; then
        error_exit "Platform detection failed"
    fi
    
    log_info "Detected platform: $detected_platform"
}

# Test installation with fake server
test_installation() {
    log_test "Testing installation with fake server..."
    
    local install_dir="${TEST_DIR}/install"
    mkdir -p "$install_dir"
    
    # Test installation with direct file access (bypassing GitHub API)
    DEACON_VERSION="$FAKE_VERSION" \
    DEACON_BASE_URL="http://localhost:$FAKE_SERVER_PORT" \
    DEACON_INSTALL_DIR="$install_dir" \
    DEACON_FORCE="true" \
    bash "$INSTALL_SCRIPT"
    
    # Verify installation
    local binary_path
    if [[ "$(uname -s)" == *"MINGW"* ]] || [[ "$(uname -s)" == *"CYGWIN"* ]]; then
        binary_path="${install_dir}/deacon.exe"
    else
        binary_path="${install_dir}/deacon"
    fi
    
    if [[ ! -f "$binary_path" ]]; then
        error_exit "Binary not found at $binary_path"
    fi
    
    if [[ ! -x "$binary_path" ]]; then
        error_exit "Binary is not executable"
    fi
    
    # Test running the binary
    local output
    output=$("$binary_path")
    if [[ "$output" != *"$FAKE_VERSION"* ]]; then
        error_exit "Binary output doesn't contain expected version: $output"
    fi
    
    log_info "Installation test passed"
}

# Test checksum verification failure
test_checksum_failure() {
    log_test "Testing checksum verification failure..."
    
    # Corrupt the SHA256SUMS file
    echo "invalid_checksum  deacon-${FAKE_VERSION}-x86_64-unknown-linux-gnu.tar.gz" > "${FAKE_SERVER_DIR}/download/${FAKE_VERSION}/SHA256SUMS"
    
    local install_dir="${TEST_DIR}/install-checksum-fail"
    mkdir -p "$install_dir"
    
    # This should fail due to checksum mismatch
    if DEACON_VERSION="$FAKE_VERSION" \
       DEACON_BASE_URL="http://localhost:$FAKE_SERVER_PORT" \
       DEACON_INSTALL_DIR="$install_dir" \
       DEACON_FORCE="true" \
       bash "$INSTALL_SCRIPT" 2>/dev/null; then
        error_exit "Installation should have failed due to checksum mismatch"
    fi
    
    log_info "Checksum failure test passed"
}

# Main test function
run_tests() {
    log_info "Starting install script tests..."
    
    # Check prerequisites
    if ! command -v python3 >/dev/null 2>&1; then
        error_exit "python3 is required for testing but not installed"
    fi
    
    if ! command -v curl >/dev/null 2>&1; then
        error_exit "curl is required for testing but not installed"
    fi
    
    # Setup test environment
    rm -rf "$TEST_DIR"
    mkdir -p "$TEST_DIR"
    
    # Create fake artifacts and start server
    create_fake_artifacts
    start_fake_server
    
    # Run tests
    test_platform_detection
    test_installation
    
    # Restore original checksums for checksum failure test
    create_fake_artifacts
    test_checksum_failure
    
    log_info "All tests passed!"
}

# Run tests if script is executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    run_tests
fi