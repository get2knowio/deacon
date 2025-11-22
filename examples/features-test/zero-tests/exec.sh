#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Zero tests with non-matching filter (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --filter nonexistent "$@"

echo "== Zero tests with missing feature (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --features missing --json "$@"
