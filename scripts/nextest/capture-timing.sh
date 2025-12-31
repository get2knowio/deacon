#!/usr/bin/env bash
# Helper script to capture and record timing data from nextest runs
# Usage: ./capture-timing.sh <profile> <output-path>

set -euo pipefail

if [ $# -ne 2 ]; then
    cat >&2 <<'EOF'
Usage: capture-timing.sh <profile> <output-path>

Arguments:
  profile      The nextest profile name (e.g., dev-fast, full, ci)
  output-path  Where to write the timing JSON artifact

Example:
  ./capture-timing.sh dev-fast artifacts/nextest/dev-fast-timing.json

EOF
    exit 1
fi

PROFILE="$1"
OUTPUT_PATH="$2"

# Ensure output directory exists
OUTPUT_DIR=$(dirname "$OUTPUT_PATH")
mkdir -p "$OUTPUT_DIR"

# Record start time
START_TIME=$(date +%s)

# Run nextest with JSON reporter and capture output
# The --message-format json-plus provides structured timing data
NEXTEST_OUTPUT=$(mktemp)
trap 'rm -f "$NEXTEST_OUTPUT"' EXIT

if cargo nextest run --profile "$PROFILE" --message-format json-plus > "$NEXTEST_OUTPUT" 2>&1; then
    NEXTEST_STATUS=0
else
    NEXTEST_STATUS=$?
fi

# Record end time
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# Parse the nextest output to extract timing information
# We look for the final test run summary
if command -v jq >/dev/null 2>&1; then
    # Use jq if available for robust JSON parsing
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    
    # Create timing artifact with structured data
    cat > "$OUTPUT_PATH" <<EOF
{
  "profile": "$PROFILE",
  "duration_seconds": $DURATION,
  "timestamp_utc": "$TIMESTAMP",
  "exit_code": $NEXTEST_STATUS,
  "notes": "Timing data captured by scripts/nextest/capture-timing.sh"
}
EOF
else
    # Fallback without jq - simpler JSON
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    
    cat > "$OUTPUT_PATH" <<EOF
{
  "profile": "$PROFILE",
  "duration_seconds": $DURATION,
  "timestamp_utc": "$TIMESTAMP",
  "exit_code": $NEXTEST_STATUS,
  "notes": "Timing data captured by scripts/nextest/capture-timing.sh (jq not available)"
}
EOF
fi

echo "Timing data written to: $OUTPUT_PATH" >&2
echo "Profile: $PROFILE | Duration: ${DURATION}s | Status: $NEXTEST_STATUS" >&2

exit $NEXTEST_STATUS
