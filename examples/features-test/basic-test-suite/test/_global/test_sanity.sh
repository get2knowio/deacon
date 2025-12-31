#!/bin/bash
# Global scenario test - collection-wide sanity check
set -e

echo "Running global sanity check..."

# Verify basic system utilities are available
if ! command -v bash >/dev/null 2>&1; then
    echo "ERROR: bash not available"
    exit 1
fi

if ! command -v sh >/dev/null 2>&1; then
    echo "ERROR: sh not available"
    exit 1
fi

# Check that /etc directory exists and is readable
if [ ! -d /etc ]; then
    echo "ERROR: /etc directory not found"
    exit 1
fi

echo "✓ Global sanity check passed"
exit 0
