#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Compose configuration (README: Run) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .
