#!/usr/bin/env bash
set -euo pipefail

echo "::error::Deprecated: .github/workflow-scripts/lib/gh_graphql.sh is no longer used. GraphQL calls are made directly from Python (intake_poller.py)." >&2
exit 1
