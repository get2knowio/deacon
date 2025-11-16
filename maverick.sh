#!/usr/bin/env bash
set -euo pipefail

# Resolve the repo that should receive Maverick's workflow execution
REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
MAVERICK_PROJECT_PATH="${MAVERICK_PROJECT_PATH:-/home/vscode/maverick}"

# Make sure uv is present before continuing
if ! command -v uv >/dev/null 2>&1; then
  echo "uv is required but was not found in PATH." >&2
  exit 1
fi

# Install the Temporal CLI when missing
if ! command -v temporal >/dev/null 2>&1; then
  echo "Temporal CLI not found; installing..."
  curl -sSf https://temporal.download/cli.sh | sh
fi

# Ensure the freshly installed CLI is discoverable
export PATH="$HOME/.temporalio/bin:$PATH"

if ! command -v temporal >/dev/null 2>&1; then
  echo "Temporal CLI installation failed (still not in PATH)." >&2
  exit 1
fi

if [ ! -d "$MAVERICK_PROJECT_PATH" ]; then
  echo "Maverick source directory not found at '$MAVERICK_PROJECT_PATH'." >&2
  echo "Set MAVERICK_PROJECT_PATH to the mounted location when invoking this script." >&2
  exit 1
fi

cd "$REPO_ROOT"

uv run --project "$MAVERICK_PROJECT_PATH" maverick "$@"
