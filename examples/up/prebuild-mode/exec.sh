#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
IMAGE_TAG="prebuild-mode:prebuild"

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 (or python) is required to parse JSON output" >&2
		exit 1
	fi
fi

# README mapping:
# - "Step 1: Create Prebuild Image" -> --prebuild run (stops after onCreate/updateContent)
# - "Step 2: Commit Prebuild Image" -> manual docker commit (left to user)
# - "Step 3: Use Prebuild Image"    -> normal up run (optional here)
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

# Step 1: prebuild (README).
echo "Running prebuild to stop after onCreate/updateContent..." >&2
prebuild_output="$( run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --prebuild "$@" )"
container_id="$(extract_container_id "$prebuild_output")"
echo "Prebuild container: ${container_id}" >&2

# Step 2: commit prebuild image (local only, mirrors README "Commit Prebuild Image").
if [ -n "$container_id" ]; then
	echo "Committing prebuild container to image ${IMAGE_TAG} ..." >&2
	docker commit "$container_id" "$IMAGE_TAG" >/dev/null
else
	echo "Skipping commit; no container ID produced" >&2
fi

if [ "${RUN_NORMAL_AFTER_PREBUILD:-true}" = "true" ]; then
	# Step 3: normal run to complete postCreate/postStart/postAttach (README).
	echo "Running normal up to finish lifecycle hooks..." >&2
	normal_output="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
	normal_container="$(extract_container_id "$normal_output")"
	if [ -n "$normal_container" ]; then
		docker rm -f "$normal_container" >/dev/null 2>&1 || true
	fi
fi

# Cleanup containers and image.
if [ -n "$container_id" ]; then
	docker rm -f "$container_id" >/dev/null 2>&1 || true
fi
docker rmi "$IMAGE_TAG" >/dev/null 2>&1 || true
