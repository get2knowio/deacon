#!/bin/bash
# Scenario test script for hello-world scenarios
set -e

SCENARIO_NAME="${1:-default}"
echo "Running scenario: ${SCENARIO_NAME}"

# Check installation succeeded
if [ ! -f /usr/local/hello-message.txt ]; then
    echo "ERROR: Feature not installed correctly"
    exit 1
fi

case "$SCENARIO_NAME" in
    "default")
        # Default greeting should be "Hello"
        if ! grep -q "Hello, World!" /usr/local/hello-message.txt; then
            echo "ERROR: Default greeting not found"
            exit 1
        fi
        ;;
    "alpine")
        # Custom greeting should be "Greetings"
        if ! grep -q "Greetings, World!" /usr/local/hello-message.txt; then
            echo "ERROR: Custom greeting not found"
            exit 1
        fi
        ;;
esac

echo "✓ Scenario test passed: ${SCENARIO_NAME}"
exit 0
