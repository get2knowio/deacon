#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LEAF_DIR="${ROOT_DIR}/leaf"

echo "== Extends chain via workspace-folder discovery (README: leaf directory) ==" >&2
(cd "$LEAF_DIR" && cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" "$@" | jq .)

echo "== Extends chain with explicit config path (README: target leaf config explicitly) ==" >&2
cargo run -p deacon -- read-configuration \
	--workspace-folder "$LEAF_DIR" \
	--config "$LEAF_DIR/devcontainer.json" \
	"$@" | jq .
