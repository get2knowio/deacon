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

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Start dev container (README: step 1) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@"

echo "== Execute with workspace discovery (README: step 2) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" node --version

echo "== Run application via workspace discovery (README: step 3) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" node /workspace/src/main.js

echo "== Execute from parent directory with absolute path (README: step 4) ==" >&2
PARENT_DIR="$(dirname "$SCRIPT_DIR")"
run bash -c "cd \"$PARENT_DIR\" && \"$DEACON_BIN\" exec --workspace-folder \"$SCRIPT_DIR\" npm --version"

echo "== Check workspace mapping (README: step 5) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" pwd
