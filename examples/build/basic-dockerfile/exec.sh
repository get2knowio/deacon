#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup_images() {
	docker images --filter "label=example.type=basic-dockerfile" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
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
trap 'cleanup_images; clean_workspace_artifacts' EXIT

cd "$SCRIPT_DIR"

echo "== Basic build (README: Basic Build) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" "$@"

echo "== Build with custom build arg (README: Build with Custom Build Args) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --build-arg FOO=BAR "$@"

echo "== Build with JSON output (README: Build with JSON Output) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --build-arg FOO=BAR --output-format json "$@"
