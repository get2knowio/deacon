#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Dependencies complex (README: Running) ==" >&2
deacon features info dependencies "${SCRIPT_DIR}/app-feature" "$@"
