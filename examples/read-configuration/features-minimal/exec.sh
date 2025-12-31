#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Base configuration only (README: Show base configuration only) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" "$@" | jq .

echo "== Include featuresConfiguration (README: Include computed featuresConfiguration) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-features-configuration \
	"$@" | jq .

echo "== Include mergedConfiguration (README: Optionally include mergedConfiguration) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-merged-configuration \
	"$@" | jq .
