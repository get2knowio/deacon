#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Basic test suite (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" "$@"

echo "== Basic test suite JSON output ==" >&2
deacon features test "$SCRIPT_DIR" --json "$@"
