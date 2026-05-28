#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=containerUser vs remoteUser" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

assert_eq() {
	local label="$1" expected="$2" actual="$3"
	if [ "$actual" != "$expected" ]; then
		echo "FAIL: ${label} expected '${expected}', got '${actual}'" >&2
		exit 1
	fi
	echo "  ok: ${label} = ${actual}" >&2
}

cd "$SCRIPT_DIR"

echo "== Bring container up ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Scenario 1: containerUser drives PID 1 ==" >&2
pid1_user="$(docker exec "$cid" id -un)"
assert_eq containerUser root "$pid1_user"

echo "== Scenario 2: remoteUser drove postCreateCommand ==" >&2
postcreate_user="$(docker exec "$cid" cat /tmp/postcreate.user)"
assert_eq postCreateCommand vscode "$postcreate_user"

echo "== Scenario 3: deacon exec runs as remoteUser ==" >&2
# `-u` is a deacon-exec flag, so use `--` to separate command args.
exec_user="$(run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" -- id -un | tail -n1)"
assert_eq deaconExec vscode "$exec_user"

echo "All scenarios passed." >&2
