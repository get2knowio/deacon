#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG="${IMAGE_TAG:-myorg/archive:exp}"
ARCHIVE_PATH="${ARCHIVE_PATH:-${SCRIPT_DIR}/archive-image.tar}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	rm -f "$ARCHIVE_PATH"
	docker rmi -f "$IMAGE_TAG" >/dev/null 2>&1 || true
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

echo "== Build with OCI archive output (README: Usage) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name "$IMAGE_TAG" \
	--output "type=oci,dest=${ARCHIVE_PATH}" \
	--output-format json \
	"$@"

echo "== Verify archive contents (README: Verify) ==" >&2
tar tf "$ARCHIVE_PATH" | head
