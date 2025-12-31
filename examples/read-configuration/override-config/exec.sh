#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Base only (README: Base only) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .

echo "== With override applied (README: With override applied) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" \
	--override-config "$SCRIPT_DIR/override.jsonc" \
	"$@" | jq .
