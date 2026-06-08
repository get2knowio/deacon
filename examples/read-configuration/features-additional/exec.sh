#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

echo "== Additional feature with featuresConfiguration (README: Include featuresConfiguration) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-features-configuration \
	--additional-features '{"./extra-feature": {"flag": true}}' \
	"$@" | jq .

echo "== Additional feature with mergedConfiguration (README: Optionally include mergedConfiguration) ==" >&2
"$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-merged-configuration \
	--additional-features '{"./extra-feature": {}}' \
	"$@" | jq .
