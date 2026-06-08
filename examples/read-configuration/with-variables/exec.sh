#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== With variables: default env (README: Without setting env var) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .

echo "== With variables: override via MY_VAR (README: With env var influencing substitution) ==" >&2
MY_VAR=hello "$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .
