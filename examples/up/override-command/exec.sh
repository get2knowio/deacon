#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=overrideCommand: false" --format '{{.ID}}' | head -n1
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

echo "== Scenario 1: overrideCommand=false runs image CMD ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
# Give the image's CMD a moment to drop the marker.
for _ in 1 2 3 4 5; do
	if docker exec "$cid" test -f /tmp/image.cmd; then break; fi
	sleep 0.3
done
docker exec "$cid" test -f /tmp/image.cmd \
	|| { echo "FAIL: image CMD did not run (no /tmp/image.cmd)" >&2; exit 1; }
echo "  ok: image CMD ran (/tmp/image.cmd present)" >&2

echo "== Scenario 2: overrideCommand=true suppresses image CMD ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--merge-config ./override.true.json "$@" >/dev/null
cid="$(container_id)"
sleep 1
if docker exec "$cid" test -f /tmp/image.cmd; then
	echo "FAIL: with overrideCommand=true, /tmp/image.cmd should be absent" >&2
	exit 1
fi
echo "  ok: image CMD overridden (/tmp/image.cmd absent)" >&2

echo "All scenarios passed." >&2
