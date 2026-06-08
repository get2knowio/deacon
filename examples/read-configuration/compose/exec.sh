#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== Compose configuration (README: Run) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .
