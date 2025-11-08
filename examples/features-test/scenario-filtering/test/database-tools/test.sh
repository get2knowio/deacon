#!/bin/bash
set -e
[ -f /usr/local/etc/database-tools.conf ] || exit 1
echo "✓ database-tools test passed"
