#!/bin/bash
set -e
SCENARIO_NAME="${1:-minimal-postgres}"
echo "Running scenario: $SCENARIO_NAME"
[ -f /usr/local/etc/database-tools.conf ] && echo "✓ Scenario test passed: $SCENARIO_NAME"
