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

# README mapping:
# - "Basic Build and Up"          -> default path
# - "Build with No Cache"         -> BUILD_NO_CACHE=true
# - "With BuildKit"               -> BUILDKIT_MODE=auto|never
# - "With Build Cache Options"    -> CACHE_FROM / CACHE_TO
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

echo "== Basic Build and Up ==" >&2
out_basic="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
cleanup_container "$(extract_container_id "$out_basic")"

echo "== Build with No Cache ==" >&2
out_no_cache="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--build-no-cache \
	"$@")"
cleanup_container "$(extract_container_id "$out_no_cache")"

echo "== With BuildKit (auto) ==" >&2
out_buildkit="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--buildkit auto \
	"$@")"
cleanup_container "$(extract_container_id "$out_buildkit")"

# Prepare a local cache directory to exercise cache-from/cache-to without registry access.
CACHE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/deacon-build-cache.XXXX")"
echo "== With Build Cache Options (local cache) ==" >&2
out_cache_to="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--buildkit auto \
	--cache-to "type=local,dest=${CACHE_DIR}" \
	"$@")"
cleanup_container "$(extract_container_id "$out_cache_to")"

# Reuse the populated cache.
out_cache_from="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--buildkit auto \
	--cache-from "type=local,src=${CACHE_DIR}" \
	"$@")"
cleanup_container "$(extract_container_id "$out_cache_from")"

rm -rf "$CACHE_DIR"
