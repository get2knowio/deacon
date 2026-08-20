#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker images --filter "label=example.type=platform-and-cache" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
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
trap 'cleanup; clean_workspace_artifacts' EXIT

cd "$SCRIPT_DIR"

echo "== Default build (README: Default Build) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" "$@"

echo "== Build without cache (README: Build Without Cache) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --no-cache "$@"

echo "== Build for specific platform (README: Build for Specific Platform) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --platform linux/amd64 "$@"
