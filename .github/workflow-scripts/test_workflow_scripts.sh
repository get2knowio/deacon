#!/usr/bin/env bash

# Test runner for workflow scripts
# Usage: ./test_workflow_scripts.sh [options]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🧪 Running workflow script tests..."
echo ""

# Check Python version
PYTHON_VERSION=$(python3 --version 2>&1 | awk '{print $2}')
echo "Python version: $PYTHON_VERSION"
echo ""

# Function to run tests for a directory
run_tests() {
    local test_dir=$1
    local test_pattern=${2:-"test_*.py"}
    
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Testing: $test_dir"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    if [ ! -d "$test_dir" ]; then
        echo -e "${YELLOW}⚠ Directory not found: $test_dir${NC}"
        return 1
    fi
    
    # Find test files
    test_files=$(find "$test_dir" -name "$test_pattern" -type f)
    
    if [ -z "$test_files" ]; then
        echo -e "${YELLOW}⚠ No test files found in $test_dir${NC}"
        return 1
    fi
    
    # Run tests with unittest
    if python3 -m unittest discover -s "$test_dir" -p "$test_pattern" -v 2>&1; then
        echo -e "${GREEN}✓ All tests passed in $test_dir${NC}"
        echo ""
        return 0
    else
        echo -e "${RED}✗ Tests failed in $test_dir${NC}"
        echo ""
        return 1
    fi
}

# Main test execution
EXIT_CODE=0

# Test maverick scripts
if ! run_tests "maverick"; then
    EXIT_CODE=1
fi

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✓ All workflow script tests passed!${NC}"
else
    echo -e "${RED}✗ Some tests failed. See output above.${NC}"
fi

exit $EXIT_CODE
