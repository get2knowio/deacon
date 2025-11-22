#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Additional feature with featuresConfiguration (README: Include featuresConfiguration) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-features-configuration \
	--additional-features '{"./extra-feature": {"flag": true}}' \
	"$@" | jq .

echo "== Additional feature with mergedConfiguration (README: Optionally include mergedConfiguration) ==" >&2
cargo run -p deacon -- read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-merged-configuration \
	--additional-features '{"./extra-feature": {}}' \
	"$@" | jq .
