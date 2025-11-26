#!/usr/bin/env bash
set -euo pipefail

# Resolve the repo that should receive Maverick's workflow execution
REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
MAVERICK_PROJECT_PATH="${MAVERICK_PROJECT_PATH:-/home/vscode/maverick}"

# Make sure node is present before continuing
if ! command -v node >/dev/null 2>&1; then
  echo "node is required but was not found in PATH." >&2
  exit 1
fi

if [ ! -d "$MAVERICK_PROJECT_PATH" ]; then
  echo "Maverick source directory not found at '$MAVERICK_PROJECT_PATH'." >&2
  echo "Set MAVERICK_PROJECT_PATH to the mounted location when invoking this script." >&2
  exit 1
fi

cd "$REPO_ROOT"

node "$MAVERICK_PROJECT_PATH/bin/maverick.mjs" "$@"
