#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Feature selection: single feature (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --features git-tools "$@"

echo "== Feature selection: multiple features (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --features git-tools python-tools "$@"
