#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== With variables: default env (README: Without setting env var) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .

echo "== With variables: override via MY_VAR (README: With env var influencing substitution) ==" >&2
MY_VAR=hello cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .
