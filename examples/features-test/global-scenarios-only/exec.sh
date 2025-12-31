#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Global scenarios only (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --global-scenarios-only "$@"
