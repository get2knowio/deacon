#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== Base only (README: Base only) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .

echo "== With override applied (README: With override applied) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--override-config "$SCRIPT_DIR/override.jsonc" \
	"$@" | jq .
