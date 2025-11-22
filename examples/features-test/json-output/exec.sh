#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== JSON output (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --json "$@"

echo "== Parse each entry with jq (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --json "$@" | jq '.[]'
