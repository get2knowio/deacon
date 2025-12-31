#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Complex graph (README: How to run) ==" >&2
deacon features plan --config "$SCRIPT_DIR/devcontainer.json" --json "$@"
