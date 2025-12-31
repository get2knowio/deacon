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
	rm -f "${SCRIPT_DIR}/shared-workspace/created-by-exec.txt" "${SCRIPT_DIR}/shared-workspace/test.txt"
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Start dev container (README: Basic User Verification) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@"

echo "== Verify user (whoami) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" whoami

echo "== Check user details (README: Check User Details) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/user-check.sh

echo "== User ID and groups (README: User ID and Groups) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" id

echo "== Home directory (README: Home Directory) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash -c 'echo $HOME && ls -la $HOME | head'

echo "== File creation and ownership (README: File Creation and Ownership) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" touch /workspace/shared-workspace/created-by-exec.txt
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" ls -l /workspace/shared-workspace/

echo "== Override user to root (README: Compare with Root User) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" --user root bash -c 'whoami && id'

echo "== User-specific environment (README: User-Specific Environment) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" env | grep -E "(USER|HOME|SHELL|USER_TYPE)"

echo "== Permission testing (README: Permission Testing) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" ls -la /home/node/ | head
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash -c 'echo \"test\" > /workspace/shared-workspace/test.txt && cat /workspace/shared-workspace/test.txt'
