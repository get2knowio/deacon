#!/bin/bash
set -e
echo "Checking environment..."
command -v bash >/dev/null 2>&1 || exit 1
[ -d /etc ] || exit 1
echo "✓ Environment check passed"
