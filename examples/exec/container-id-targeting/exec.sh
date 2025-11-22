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

CID="$(container_id)"
if [ -z "$CID" ]; then
	echo "Container not found after up" >&2
	exit 1
fi
echo "Using container: ${CID}" >&2

echo "== Execute simple command via --container-id (README: step 3) ==" >&2
run "$DEACON_BIN" exec --container-id "$CID" echo "Hello from container"

echo "== Run test script via --container-id (README: step 4) ==" >&2
run "$DEACON_BIN" exec --container-id "$CID" bash /workspace/test-script.sh

echo "== Check environment variables (README: step 5) ==" >&2
run "$DEACON_BIN" exec --container-id "$CID" env | grep CONTAINER_
