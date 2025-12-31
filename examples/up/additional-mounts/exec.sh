#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 (or python) is required to parse JSON output" >&2
		exit 1
	fi
fi

# README sections: "Basic (Config File Mounts)", "Add Runtime Mounts", "Multiple Mount Flags", "External Volumes".
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

cleanup_container() {
	if [ -n "$1" ]; then
		docker rm -f "$1" >/dev/null 2>&1 || true
	fi
}

cd "$SCRIPT_DIR"

echo "== Basic (Config File Mounts) from devcontainer.json ==" >&2
out_basic="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
cleanup_container "$(extract_container_id "$out_basic")"
docker volume rm myapp-cache >/dev/null 2>&1 || true

echo "== Add Runtime Mounts / Multiple Mount Flags ==" >&2
out_runtime="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--mount "type=bind,source=${SCRIPT_DIR}/config,target=/tmp/myapp-config,readonly" \
	--mount "type=volume,source=additional-cache,target=/var/cache/additional" \
	"$@")"
cleanup_container "$(extract_container_id "$out_runtime")"
docker volume rm additional-cache >/dev/null 2>&1 || true
docker volume rm myapp-cache >/dev/null 2>&1 || true

echo "== External Volumes example ==" >&2
created_shared_data=0
if ! docker volume inspect shared-data >/dev/null 2>&1; then
	docker volume create shared-data >/dev/null
	created_shared_data=1
fi
out_external="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--mount "type=volume,source=shared-data,target=/data,external=true" \
	"$@")"
cleanup_container "$(extract_container_id "$out_external")"
if [ "$created_shared_data" -eq 1 ]; then
	docker volume rm shared-data >/dev/null 2>&1 || true
fi
docker volume rm myapp-cache >/dev/null 2>&1 || true
