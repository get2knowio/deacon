#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Local feature install" --format '{{.ID}}' | head -n1
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

chmod +x "$SCRIPT_DIR"/hello-feature/install.sh

cd "$SCRIPT_DIR"

# Top-level devcontainer.json so the relative `./hello-feature` path resolves
# against the config dir; point deacon at it explicitly.
echo "== Bring container up (installs a local ./hello-feature) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
[ -n "$cid" ] || { echo "FAIL: container not found after up" >&2; exit 1; }

echo "== Scenario 1: the local feature's marker is baked into the image ==" >&2
marker="$(docker exec "$cid" cat /usr/local/share/local-feature/marker 2>/dev/null || true)"
echo "  marker: ${marker}" >&2
[ -n "$marker" ] || { echo "FAIL: local feature did not run (marker missing)" >&2; exit 1; }
echo "  ok: local feature installed" >&2

echo "== Scenario 2: the feature option (greeting=bonjour) was applied ==" >&2
case "$marker" in
	"bonjour from local feature v1.0.0")
		echo "  ok: option override honored" >&2
		;;
	*)
		echo "FAIL: expected greeting 'bonjour', got: ${marker}" >&2
		exit 1
		;;
esac

echo "All scenarios passed." >&2
