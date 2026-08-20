#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run_expect_fail() {
	echo "+ $*" >&2
	set +e
	"$@"
	code=$?
	set -e
	if [ $code -eq 0 ]; then
		echo "Expected failure but command succeeded" >&2
		exit 1
	fi
	echo "Received expected failure (exit $code)" >&2
}

cleanup() {
	chmod 700 "$SCRIPT_DIR/readonly" >/dev/null 2>&1 || true
	rm -rf "$SCRIPT_DIR/readonly"
	docker rmi -f myorg/unwritable:latest >/dev/null 2>&1 || true
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

mkdir -p readonly
chmod 500 readonly

echo "== Unwritable output destination (README: expect error) ==" >&2
run_expect_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name myorg/unwritable:latest \
	--output type=oci,dest=readonly/image.tar "$@"
