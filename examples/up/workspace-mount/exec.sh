#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Custom workspaceMount" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Bring container up ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Scenario 1: workspaceFolder honored (postCreate ran in /srv/app) ==" >&2
pwd_value="$(docker exec "$cid" cat /tmp/pwd | tr -d '\n')"
[ "$pwd_value" = "/srv/app" ] \
	|| { echo "FAIL: expected pwd=/srv/app, got '${pwd_value}'" >&2; exit 1; }
echo "  ok: pwd=${pwd_value}" >&2

echo "== Scenario 2: docker inspect shows the mount at /srv/app ==" >&2
mount_json="$(docker inspect "$cid" --format '{{json .Mounts}}')"
echo "$mount_json" | grep -q '"Destination":"/srv/app"' \
	|| { echo "FAIL: /srv/app destination missing from mounts" >&2; echo "$mount_json" >&2; exit 1; }
echo "  ok: bind mount destination = /srv/app" >&2

echo "== Scenario 3: workspace files visible inside container ==" >&2
docker exec "$cid" cat /tmp/listing >&2
docker exec "$cid" test -f /srv/app/README.md \
	|| { echo "FAIL: README.md missing from /srv/app" >&2; exit 1; }
echo "  ok: README.md present at /srv/app" >&2

echo "All scenarios passed." >&2
