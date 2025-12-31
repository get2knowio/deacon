#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Manifest from local feature (README: Running) ==" >&2
deacon features info manifest "${SCRIPT_DIR}/sample-feature" "$@"

echo "== Manifest from local feature (JSON output) ==" >&2
deacon features info manifest "${SCRIPT_DIR}/sample-feature" --output-format json "$@"
