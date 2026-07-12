#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

if [ "$(uname -s)" != "Linux" ]; then
	echo "SKIP: updateRemoteUserUID is Linux-only" >&2
	exit 0
fi

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=updateRemoteUserUID: true" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

host_uid="$(id -u)"
host_gid="$(id -g)"

cd "$SCRIPT_DIR"

echo "== Scenario 1: updateRemoteUserUID: true (sync to host UID ${host_uid}) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
container_uid="$(docker exec "$cid" cat /tmp/uid | tr -d '\n')"
container_gid="$(docker exec "$cid" cat /tmp/gid | tr -d '\n')"
echo "  host UID/GID: ${host_uid}/${host_gid}" >&2
echo "  container UID/GID: ${container_uid}/${container_gid}" >&2
[ "$container_uid" = "$host_uid" ] \
	|| { echo "FAIL: expected container UID=${host_uid}, got ${container_uid}" >&2; exit 1; }
echo "  ok: container UID matches host" >&2

echo "== Scenario 2: updateRemoteUserUID: false (keeps image UID 5000) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--merge-config ./override.disable.json "$@" >/dev/null
cid="$(container_id)"
container_uid="$(docker exec "$cid" cat /tmp/uid | tr -d '\n')"
echo "  container UID (no sync): ${container_uid}" >&2
[ "$container_uid" = "5000" ] \
	|| { echo "FAIL: expected container UID=5000, got ${container_uid}" >&2; exit 1; }
echo "  ok: UID preserved from image" >&2

echo "All scenarios passed." >&2
