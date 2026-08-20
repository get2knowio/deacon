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

# README: "Basic Up Command" — simplest image-based container creation.
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" -c 'import json, sys; data = json.load(sys.stdin); print(data.get("containerId", ""))'
}

# Runtime artifacts `deacon up` / `deacon build` may write into this example's
# workspace. Removing only these generated paths leaves the directory exactly as
# committed (#179); the committed `.devcontainer/` config is never touched.
clean_workspace_artifacts() {
	rm -rf \
		"${SCRIPT_DIR}/.devcontainer-state" \
		"${SCRIPT_DIR}/.devcontainer/build-cache" \
		"${SCRIPT_DIR}/.deacon" \
		"${SCRIPT_DIR}/.deacon-temp-build"
	# The lockfile sits beside the config and gains a leading dot when the
	# config basename has one (`.devcontainer.json` -> `.devcontainer-lock.json`).
	rm -f \
		"${SCRIPT_DIR}/devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer/devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer/.devcontainer-lock.json"
}
trap clean_workspace_artifacts EXIT

cd "$SCRIPT_DIR"
echo "== Basic Image: create container and run postCreateCommand (README: Basic Up Command) ==" >&2
output="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
container_id="$(extract_container_id "$output")"
echo "Container: ${container_id}" >&2

if [ -n "$container_id" ]; then
	docker rm -f "$container_id" >/dev/null 2>&1 || true
fi
