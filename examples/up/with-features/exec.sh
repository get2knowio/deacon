#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

# README mapping:
# - "Basic Up with Features" -> default run
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	local json_out=""
	if command -v "$PYTHON_BIN" >/dev/null 2>&1; then
		json_out="$(printf '%s' "$1" | "$PYTHON_BIN" - <<'PY'
import json, sys
lines = [ln.strip() for ln in sys.stdin.read().splitlines() if ln.strip()]
if lines:
    try:
        data = json.loads(lines[-1])
        cid = data.get("containerId", "")
        if cid:
            print(cid)
            sys.exit(0)
    except json.JSONDecodeError:
        pass
sys.exit(0)
PY
)" || json_out=""
		if [ -n "$json_out" ]; then
			printf '%s' "$json_out"
			return 0
		fi
	fi

	local fallback=""
	fallback="$(docker ps -aq --filter "label=devcontainer.local_folder=$SCRIPT_DIR" --latest 2>/dev/null || true)"
	printf '%s' "$fallback" | head -n1
}

cd "$SCRIPT_DIR"

echo "== Basic Up with Features ==" >&2
out_basic="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --skip-post-create "$@")"
basic_container="$(extract_container_id "$out_basic")"

echo "== With Additional Features ==" >&2
out_additional="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --skip-post-create \
	--additional-features '{
		"ghcr.io/devcontainers/features/docker-in-docker:2": {
			"version": "latest"
		}
	}' \
	"$@")"
additional_container="$(extract_container_id "$out_additional")"

echo "== Skip Feature Auto-Mapping ==" >&2
out_skip_map="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --skip-post-create \
	--skip-feature-auto-mapping \
	"$@")"
skip_map_container="$(extract_container_id "$out_skip_map")"

# Cleanup containers.
for cid in "$basic_container" "$additional_container" "$skip_map_container"; do
	if [ -n "$cid" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
done
