#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature dependency ordering" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	rm -f "$SCRIPT_DIR/devcontainer-lock.json"
}
trap cleanup EXIT

chmod +x "$SCRIPT_DIR"/feature-*/install.sh

cd "$SCRIPT_DIR"

# Top-level devcontainer.json so the `./feature-*` paths resolve against the
# config dir; point deacon at it explicitly.
echo "== Bring container up (no overrideFeatureInstallOrder; deps decide order) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
[ -n "$cid" ] || { echo "FAIL: container not found after up" >&2; exit 1; }

echo "== Scenario: installsAfter + dependsOn drive the install order ==" >&2
# Declaration/alphabetical default would be app,base,lib. The dependency graph
# (lib installsAfter base; app dependsOn lib) forces base -> lib -> app.
actual="$(docker exec "$cid" cat /usr/local/share/feature-order/log 2>/dev/null | tr '\n' ',' | sed 's/,$//')"
expected="base,lib,app"
echo "  expected: ${expected}" >&2
echo "  actual:   ${actual}" >&2
[ "$actual" = "$expected" ] \
	|| { echo "FAIL: dependency-driven install order not honored" >&2; exit 1; }
echo "  ok: dependsOn/installsAfter honored" >&2

echo "All scenarios passed." >&2
