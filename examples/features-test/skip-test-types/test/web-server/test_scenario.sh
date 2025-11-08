#!/bin/bash
set -e
SCENARIO="${1:-nginx}"
echo "Running scenario: $SCENARIO"
[ -f /usr/local/etc/web-server.conf ] && echo "✓ Scenario test passed"
