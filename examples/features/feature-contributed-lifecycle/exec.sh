#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature-contributed lifecycle commands" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

chmod +x "$SCRIPT_DIR"/monitor/install.sh "$SCRIPT_DIR"/tooling/install.sh

cd "$SCRIPT_DIR"

echo "== Bring container up ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Read lifecycle log ==" >&2
docker exec "$cid" cat /tmp/lifecycle.log | sed 's/^/  | /' >&2

echo "== Scenario 1: every postCreate contributor fired ==" >&2
for entry in user-postcreate monitor-postcreate tooling-postcreate; do
	docker exec "$cid" grep -q "^${entry}$" /tmp/lifecycle.log \
		|| { echo "FAIL: '${entry}' missing from lifecycle.log" >&2; exit 1; }
	echo "  ok: ${entry}" >&2
done

echo "== Scenario 2: feature-contributed postStart fired ==" >&2
# postStart may fire after a brief delay; give it a moment.
for _ in 1 2 3 4 5; do
	if docker exec "$cid" grep -q '^monitor-poststart$' /tmp/lifecycle.log; then
		echo "  ok: monitor-poststart" >&2
		break
	fi
	sleep 0.5
done
docker exec "$cid" grep -q '^monitor-poststart$' /tmp/lifecycle.log \
	|| { echo "FAIL: monitor-poststart missing" >&2; exit 1; }

echo "All scenarios passed." >&2
