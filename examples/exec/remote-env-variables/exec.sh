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

echo "== Default environment from config (README: step 2) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/check-env.sh

echo "== Override config variable via CLI (README: step 3) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--remote-env APP_MODE=production \
	bash /workspace/check-env.sh

echo "== Add new variables via CLI (README: step 4) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--remote-env CUSTOM_VAR=custom-value \
	--remote-env DEBUG=true \
	env | grep -E "(CUSTOM_VAR|DEBUG)"

echo "== Set empty value (README: step 5) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--remote-env EMPTY_VAR= \
	env | grep EMPTY_VAR

echo "== Multiple remote-env flags to test order (README: step 6) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--remote-env FOO=first \
	--remote-env FOO=second \
	bash -c 'echo $FOO'
