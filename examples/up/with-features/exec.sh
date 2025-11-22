#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

# README mapping:
# - "Basic Up with Features"       -> default run
# - "With Additional Features"     -> set ADDITIONAL_FEATURES_JSON
# - "Skip Feature Auto-Mapping"    -> SKIP_FEATURE_AUTO_MAPPING=true
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" - <<'PY'
import json, sys
data = json.load(sys.stdin)
print(data.get("containerId", ""))
PY
}

cd "$SCRIPT_DIR"

echo "== Basic Up with Features ==" >&2
out_basic="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
basic_container="$(extract_container_id "$out_basic")"

echo "== With Additional Features ==" >&2
out_additional="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--additional-features '{
		"ghcr.io/devcontainers/features/docker-in-docker:2": {
			"version": "latest"
		}
	}' \
	"$@")"
additional_container="$(extract_container_id "$out_additional")"

echo "== Skip Feature Auto-Mapping ==" >&2
out_skip_map="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--skip-feature-auto-mapping \
	"$@")"
skip_map_container="$(extract_container_id "$out_skip_map")"

# Cleanup containers.
for cid in "$basic_container" "$additional_container" "$skip_map_container"; do
	if [ -n "$cid" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
done
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 (or python) is required to parse JSON output" >&2
		exit 1
	fi
fi
