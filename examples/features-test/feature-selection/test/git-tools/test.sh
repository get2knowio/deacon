#!/bin/bash
set -e
[ -f /usr/local/etc/git-tools.conf ] || exit 1
grep -q "GIT_TOOLS_VERSION" /usr/local/etc/git-tools.conf || exit 1
echo "✓ git-tools test passed"
