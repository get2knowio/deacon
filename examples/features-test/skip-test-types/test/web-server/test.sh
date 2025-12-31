#!/bin/bash
set -e
[ -f /usr/local/etc/web-server.conf ] || exit 1
echo "✓ web-server test passed"
