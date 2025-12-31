#!/bin/bash
# Scenario test script for color-tools scenarios
set -e

SCENARIO_NAME="${1:-with-blue}"
echo "Running scenario: ${SCENARIO_NAME}"

# Check installation succeeded
if [ ! -f /usr/local/etc/color-config.conf ]; then
    echo "ERROR: Feature not installed correctly"
    exit 1
fi

case "$SCENARIO_NAME" in
    "with-red")
        if ! grep -q "FAVORITE_COLOR=red" /usr/local/etc/color-config.conf; then
            echo "ERROR: Red color not configured"
            exit 1
        fi
        ;;
    "with-blue")
        if ! grep -q "FAVORITE_COLOR=blue" /usr/local/etc/color-config.conf; then
            echo "ERROR: Blue color not configured"
            exit 1
        fi
        ;;
esac

echo "✓ Scenario test passed: ${SCENARIO_NAME}"
exit 0
