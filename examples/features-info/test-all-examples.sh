#!/bin/bash
# test-all-examples.sh - Run all features-info examples
# Usage: cd examples/features-info && bash test-all-examples.sh

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=================================="
echo "Features Info Examples Test Suite"
echo "=================================="
echo ""

# Check if deacon is available
if ! command -v deacon &> /dev/null; then
    echo -e "${RED}✗ deacon command not found${NC}"
    echo "  Build it with: cargo build --release"
    exit 1
fi

# Check if network tests should run
NETWORK_ENABLED="${DEACON_NETWORK_TESTS:-0}"
if [ "$NETWORK_ENABLED" != "1" ]; then
    echo -e "${YELLOW}⚠ Network tests disabled${NC}"
    echo "  Set DEACON_NETWORK_TESTS=1 to enable registry examples"
    echo ""
fi

PASSED=0
FAILED=0
SKIPPED=0

test_example() {
    local name="$1"
    local dir="$2"
    local requires_network="$3"
    local command="$4"
    local expect_success="${5:-true}"
    
    echo -n "Testing: $name ... "
    
    # Skip network tests if disabled
    if [ "$requires_network" = "true" ] && [ "$NETWORK_ENABLED" != "1" ]; then
        echo -e "${YELLOW}SKIPPED${NC} (requires network)"
        SKIPPED=$((SKIPPED + 1))
        return
    fi
    
    # Run test
    cd "$dir"
    if eval "$command" > /dev/null 2>&1; then
        if [ "$expect_success" = "true" ]; then
            echo -e "${GREEN}✓ PASSED${NC}"
            PASSED=$((PASSED + 1))
        else
            echo -e "${RED}✗ FAILED${NC} (expected failure but succeeded)"
            FAILED=$((FAILED + 1))
        fi
    else
        EXIT_CODE=$?
        if [ "$expect_success" = "false" ]; then
            echo -e "${GREEN}✓ PASSED${NC} (correctly failed)"
            PASSED=$((PASSED + 1))
        else
            echo -e "${RED}✗ FAILED${NC} (exit code: $EXIT_CODE)"
            FAILED=$((FAILED + 1))
        fi
    fi
    cd - > /dev/null
}

echo "User Story 1: Manifest and Canonical ID"
echo "----------------------------------------"
test_example "US1: Public registry text" \
    "manifest-public-registry" \
    "true" \
    "deacon features info manifest ghcr.io/devcontainers/features/node:1"

test_example "US1: Public registry JSON" \
    "manifest-json-output" \
    "true" \
    "deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json | jq -e '.manifest and .canonicalId'"

test_example "US1: Local feature text" \
    "manifest-local-feature" \
    "false" \
    "deacon features info manifest ./sample-feature"

test_example "US1: Local feature JSON" \
    "manifest-local-feature" \
    "false" \
    "deacon features info manifest ./sample-feature --output-format json | jq -e '.canonicalId == null'"

echo ""
echo "User Story 2: Published Tags"
echo "-----------------------------"
test_example "US2: Tags text" \
    "tags-public-feature" \
    "true" \
    "deacon features info tags ghcr.io/devcontainers/features/node"

test_example "US2: Tags JSON" \
    "tags-json-output" \
    "true" \
    "deacon features info tags ghcr.io/devcontainers/features/node --output-format json | jq -e '.publishedTags | length > 0'"

echo ""
echo "User Story 3: Dependency Graph"
echo "--------------------------------"
test_example "US3: Simple dependencies" \
    "dependencies-simple" \
    "false" \
    "deacon features info dependencies ./my-feature | grep -q 'graph TD'"

test_example "US3: Complex dependencies" \
    "dependencies-complex" \
    "false" \
    "deacon features info dependencies ./app-feature | grep -q 'graph TD'"

test_example "US3: Dependencies JSON mode (should fail)" \
    "dependencies-simple" \
    "false" \
    "deacon features info dependencies ./my-feature --output-format json" \
    "false"

echo ""
echo "User Story 4: Verbose Mode"
echo "---------------------------"
test_example "US4: Verbose text" \
    "verbose-text-output" \
    "true" \
    "deacon features info verbose ghcr.io/devcontainers/features/node:1"

test_example "US4: Verbose JSON" \
    "verbose-json-output" \
    "true" \
    "deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json | jq -e '.manifest and .canonicalId and .publishedTags'"

echo ""
echo "Edge Cases"
echo "----------"
test_example "Edge: Invalid ref JSON" \
    "error-handling-invalid-ref" \
    "false" \
    "OUTPUT=\$(deacon features info manifest invalid-ref --output-format json 2>/dev/null); [ \"\$OUTPUT\" = \"{}\" ]" \
    "false"

test_example "Edge: Local feature tags (should fail)" \
    "local-feature-only-manifest" \
    "false" \
    "OUTPUT=\$(deacon features info tags ./local-feature --output-format json 2>/dev/null); [ \"\$OUTPUT\" = \"{}\" ]" \
    "false"

test_example "Edge: Local feature dependencies (should fail)" \
    "local-feature-only-manifest" \
    "false" \
    "OUTPUT=\$(deacon features info dependencies ./local-feature --output-format json 2>/dev/null); [ \"\$OUTPUT\" = \"{}\" ]" \
    "false"

echo ""
echo "=================================="
echo "Test Results"
echo "=================================="
echo -e "${GREEN}Passed:  $PASSED${NC}"
echo -e "${RED}Failed:  $FAILED${NC}"
echo -e "${YELLOW}Skipped: $SKIPPED${NC}"
echo "Total:   $((PASSED + FAILED + SKIPPED))"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    if [ $SKIPPED -gt 0 ]; then
        echo -e "${YELLOW}Note: Some tests were skipped (set DEACON_NETWORK_TESTS=1 to run them)${NC}"
    fi
    exit 0
fi
