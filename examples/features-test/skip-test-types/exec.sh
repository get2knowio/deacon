#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Skip duplicated tests (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --skip-duplicated "$@"

echo "== Skip scenarios (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --skip-scenarios "$@"
