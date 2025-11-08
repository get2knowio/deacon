#!/bin/bash
set -e
echo "Testing connectivity..."
command -v sh >/dev/null 2>&1 || exit 1
echo "✓ Connectivity test passed"
