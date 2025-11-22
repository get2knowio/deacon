#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Scenario filtering: minimal (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --filter minimal "$@"

echo "== Scenario filtering: postgres (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --filter postgres "$@"
