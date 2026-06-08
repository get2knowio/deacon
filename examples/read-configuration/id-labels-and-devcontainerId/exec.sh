#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== ID labels substitution (README: Provide id-labels) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--id-label com.example.project=rc-demo \
	--id-label "com.example.user=$(whoami)" \
	"$@" | jq .
