#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LEAF_DIR="${ROOT_DIR}/leaf"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== Extends chain via workspace-folder discovery (README: leaf directory) ==" >&2
(cd "$LEAF_DIR" && "$DEACON_BIN" read-configuration --workspace-folder "$(pwd)" "$@" | jq .)

echo "== Extends chain with explicit config path (README: target leaf config explicitly) ==" >&2
"$DEACON_BIN" read-configuration \
	--workspace-folder "$LEAF_DIR" \
	--config "$LEAF_DIR/.devcontainer.json" \
	"$@" | jq .
