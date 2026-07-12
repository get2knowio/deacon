#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== Base only (README: Base only) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .

echo "== With merge fragment applied (README: With override applied) ==" >&2
# --merge-config deep-overlays the fragment onto the base (later wins). Since
# #285, --override-config instead REPLACES the base, so this overlay demo uses
# --merge-config.
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--merge-config "$SCRIPT_DIR/override.jsonc" \
	"$@" | jq .
