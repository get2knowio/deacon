#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Legacy containerEnv normalization (README: Run) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .
