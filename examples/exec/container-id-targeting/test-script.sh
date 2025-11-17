#!/bin/bash
# Test script demonstrating exec behavior

echo "=== Container Execution Test ==="
echo "Current user: $(whoami)"
echo "Working directory: $(pwd)"
echo "Container hostname: $(hostname)"
echo "Environment variable: ${CONTAINER_ENV_VAR:-not-set}"
echo "=== Test Complete ==="
exit 0
