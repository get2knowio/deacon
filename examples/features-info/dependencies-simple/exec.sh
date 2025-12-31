#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Dependencies simple (README: Running) ==" >&2
deacon features info dependencies "${SCRIPT_DIR}/my-feature" "$@"

echo "== Dependencies JSON unsupported (README: JSON Mode Not Supported) ==" >&2
set +e
deacon features info dependencies "${SCRIPT_DIR}/my-feature" --output-format json "$@"
echo "Exit code (expected non-zero): $?" >&2
set -e
