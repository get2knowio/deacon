#!/bin/bash
# run-validation.sh
# Runs full Rust validation suite and returns structured JSON results
# Returns: {"all_passed": bool, "fmt": {...}, "clippy": {...}, "build": {...}, "test": {...}}

# Temp files for capturing output
FMT_OUT=$(mktemp)
CLIPPY_OUT=$(mktemp)
BUILD_OUT=$(mktemp)
TEST_OUT=$(mktemp)

cleanup() {
    rm -f "$FMT_OUT" "$CLIPPY_OUT" "$BUILD_OUT" "$TEST_OUT"
}
trap cleanup EXIT

# Run format check (and auto-fix)
cargo fmt --all 2>"$FMT_OUT"
FMT_STATUS=$?

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings 2>"$CLIPPY_OUT"
CLIPPY_STATUS=$?

# Run build
cargo build --all-targets 2>"$BUILD_OUT"
BUILD_STATUS=$?

# Run tests (excluding parity tests)
cargo test --all-features -- --skip parity 2>"$TEST_OUT"
TEST_STATUS=$?

# Helper to escape JSON strings
json_escape() {
    python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))" 2>/dev/null || \
    jq -Rs '.' 2>/dev/null || \
    echo '""'
}

# Build JSON output
FMT_OUTPUT=$(cat "$FMT_OUT" | json_escape)
CLIPPY_OUTPUT=$(cat "$CLIPPY_OUT" | json_escape)
BUILD_OUTPUT=$(cat "$BUILD_OUT" | json_escape)
TEST_OUTPUT=$(cat "$TEST_OUT" | json_escape)

ALL_PASSED="false"
if [ $FMT_STATUS -eq 0 ] && [ $CLIPPY_STATUS -eq 0 ] && [ $BUILD_STATUS -eq 0 ] && [ $TEST_STATUS -eq 0 ]; then
    ALL_PASSED="true"
fi

cat <<EOF
{
  "all_passed": $ALL_PASSED,
  "fmt": {
    "passed": $([ $FMT_STATUS -eq 0 ] && echo "true" || echo "false"),
    "exit_code": $FMT_STATUS,
    "output": $FMT_OUTPUT
  },
  "clippy": {
    "passed": $([ $CLIPPY_STATUS -eq 0 ] && echo "true" || echo "false"),
    "exit_code": $CLIPPY_STATUS,
    "output": $CLIPPY_OUTPUT
  },
  "build": {
    "passed": $([ $BUILD_STATUS -eq 0 ] && echo "true" || echo "false"),
    "exit_code": $BUILD_STATUS,
    "output": $BUILD_OUTPUT
  },
  "test": {
    "passed": $([ $TEST_STATUS -eq 0 ] && echo "true" || echo "false"),
    "exit_code": $TEST_STATUS,
    "output": $TEST_OUTPUT
  }
}
EOF
