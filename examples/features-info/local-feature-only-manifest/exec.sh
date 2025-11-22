#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Manifest mode (text) (README: Supported: Manifest Mode) ==" >&2
deacon features info manifest "${SCRIPT_DIR}/local-feature" "$@"

echo "== Manifest mode (json) ==" >&2
deacon features info manifest "${SCRIPT_DIR}/local-feature" --output-format json "$@"

echo "== Tags mode unsupported (text) ==" >&2
set +e
deacon features info tags "${SCRIPT_DIR}/local-feature" "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Tags mode unsupported (json) ==" >&2
deacon features info tags "${SCRIPT_DIR}/local-feature" --output-format json "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Dependencies mode unsupported ==" >&2
deacon features info dependencies "${SCRIPT_DIR}/local-feature" "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Verbose mode unsupported ==" >&2
deacon features info verbose "${SCRIPT_DIR}/local-feature" "$@"
echo "Exit code (expected 1): $?" >&2
set -e
