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

echo "== Start dev container (README: Prerequisites) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --mount-workspace-git-root false "$@"

echo "== Default probe: loginInteractiveShell (README: Test Default Probe) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/probe-test.sh

echo "== Override probe: interactiveShell (README: Override Probe Mode) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--default-user-env-probe interactiveShell \
	bash /workspace/probe-test.sh

echo "== Override probe: loginShell (README: Override Probe Mode) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--default-user-env-probe loginShell \
	bash /workspace/probe-test.sh

echo "== Override probe: none (README: Override Probe Mode) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--default-user-env-probe none \
	bash /workspace/probe-test.sh

echo "== PATH comparison across modes (README: Compare PATH Across Modes) ==" >&2
for mode in loginInteractiveShell interactiveShell loginShell none; do
	echo "--- PATH with ${mode} ---" >&2
	run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
		--default-user-env-probe "$mode" \
		bash -lc 'echo $PATH'
done

echo "== Custom variable availability (README: Check Custom Variables) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--default-user-env-probe interactiveShell \
	bash -lc 'echo "CUSTOM_VAR=${CUSTOM_VAR}"'

run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--default-user-env-probe loginShell \
	bash -lc 'echo "CUSTOM_VAR=${CUSTOM_VAR:-<not-set>}"'
