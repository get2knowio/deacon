#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=overrideFeatureInstallOrder" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

chmod +x "$SCRIPT_DIR"/feature-*/install.sh

cd "$SCRIPT_DIR"

echo "== Bring container up (features install in override order) ==" >&2
# This example keeps `devcontainer.json` at the top level (not under
# `.devcontainer/`) so the relative `./feature-*` paths resolve. Point
# deacon at it explicitly.
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Read install order log ==" >&2
actual="$(docker exec "$cid" cat /tmp/feature-order/log 2>/dev/null | tr '\n' ',' | sed 's/,$//')"
expected="charlie,alpha,bravo"
echo "  expected: ${expected}" >&2
echo "  actual:   ${actual}" >&2
[ "$actual" = "$expected" ] \
	|| { echo "FAIL: install order did not match override" >&2; exit 1; }
echo "  ok: overrideFeatureInstallOrder honored" >&2

echo "All scenarios passed." >&2
