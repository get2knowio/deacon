#!/bin/bash
set -e
[ -f /usr/local/etc/cli-tools.conf ] || exit 1
echo "✓ cli-tools test passed"
