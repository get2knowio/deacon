#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps --filter "label=devcontainer.local_folder=${SCRIPT_DIR}" --format "{{.ID}}" | head -n1
}

cleanup() {
	local cid
	cid="$(container_id)"
	if [ -n "$cid" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
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

echo "== Start dev container ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --mount-workspace-git-root false "$@"

echo "== Success (Exit 0) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" true

echo "== Standard failure (Exit 1) ==" >&2
set +e
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" false
echo "Exit code: $?" >&2
set -e

echo "== Custom exit code (README: Custom Exit Codes) ==" >&2
set +e
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/exit-codes.sh 42
echo "Exit code: $?" >&2
set -e

echo "== Command not found (Exit 127) ==" >&2
set +e
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" nonexistent-command
echo "Exit code: $?" >&2
set -e

echo "== Signal termination via timeout (README: Timeout with SIGTERM) ==" >&2
set +e
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" timeout 2s bash /workspace/timeout-test.sh
echo "Exit code: $?" >&2
set -e

echo "== Killed process via timeout -s KILL (README: Killed Process) ==" >&2
set +e
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" timeout -s KILL 2s bash /workspace/timeout-test.sh
echo "Exit code: $?" >&2
set -e

echo "== Test all scripted scenarios (README: Testing All Exit Scenarios) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/exit-codes.sh test-all
