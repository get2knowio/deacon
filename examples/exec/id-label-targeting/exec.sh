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

echo "== Execute using workspace label (README: step 2) ==" >&2
run "$DEACON_BIN" exec --id-label "devcontainer.local_folder=${SCRIPT_DIR}" python3 /workspace/app.py

echo "== Execute using custom label (README: step 3) ==" >&2
run "$DEACON_BIN" exec --id-label "app.name=exec-example" python3 --version

echo "== Multiple labels for precise targeting (README: step 4) ==" >&2
run "$DEACON_BIN" exec \
	--id-label "devcontainer.local_folder=${SCRIPT_DIR}" \
	--id-label "app.environment=development" \
	whoami

echo "== List environment variables (README: step 5) ==" >&2
run "$DEACON_BIN" exec --id-label "app.name=exec-example" env | sort
