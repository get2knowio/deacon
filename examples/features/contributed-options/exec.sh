#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
VOLUME_NAME="deacon-contrib-vol"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature-contributed container options" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	docker volume rm "$VOLUME_NAME" >/dev/null 2>&1 || true
	rm -f "$SCRIPT_DIR/devcontainer-lock.json"
}
trap cleanup EXIT

chmod +x "$SCRIPT_DIR"/probe-feature/install.sh

cd "$SCRIPT_DIR"

echo "== Bring container up (feature contributes mount/entrypoint/init/capAdd) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
[ -n "$cid" ] || { echo "FAIL: container not found after up" >&2; exit 1; }

echo "== Scenario 1: feature-contributed mount is attached ==" >&2
mounts="$(docker inspect -f '{{range .Mounts}}{{.Destination}} {{end}}' "$cid")"
echo "  mounts: ${mounts}" >&2
case " $mounts " in
	*" /contrib-data "*) echo "  ok: /contrib-data mounted" >&2 ;;
	*) echo "FAIL: feature mount /contrib-data not present" >&2; exit 1 ;;
esac

echo "== Scenario 2: feature-contributed capability (SYS_PTRACE) applied ==" >&2
caps="$(docker inspect -f '{{json .HostConfig.CapAdd}}' "$cid")"
echo "  CapAdd: ${caps}" >&2
case "$caps" in
	*SYS_PTRACE*) echo "  ok: SYS_PTRACE added" >&2 ;;
	*) echo "FAIL: SYS_PTRACE capability not added" >&2; exit 1 ;;
esac

echo "== Scenario 3: feature requested init (tini) ==" >&2
init="$(docker inspect -f '{{.HostConfig.Init}}' "$cid")"
echo "  Init: ${init}" >&2
[ "$init" = "true" ] || { echo "FAIL: HostConfig.Init expected true" >&2; exit 1; }
echo "  ok: init enabled" >&2

echo "== Scenario 4: feature-contributed entrypoint actually ran ==" >&2
ran="$(docker exec "$cid" cat /tmp/contrib-entrypoint-ran 2>/dev/null || true)"
echo "  marker: ${ran}" >&2
[ -n "$ran" ] || { echo "FAIL: feature entrypoint did not run (marker missing)" >&2; exit 1; }
echo "  ok: entrypoint chained and ran" >&2

echo "All scenarios passed." >&2
