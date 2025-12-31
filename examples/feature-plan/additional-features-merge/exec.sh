#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Additional features merge (README: How to run) ==" >&2
deacon features plan --config "$SCRIPT_DIR/devcontainer.json" \
	--additional-features '{"feature-cli": {"option": true}}' \
	--json "$@"
