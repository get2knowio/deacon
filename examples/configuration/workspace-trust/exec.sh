#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
MARKER="${SCRIPT_DIR}/initialize-ran.marker"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Workspace trust gate" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	rm -f "$MARKER"
}
trap cleanup EXIT

cd "$SCRIPT_DIR"
rm -f "$MARKER"

# `initializeCommand` runs on the HOST before any container sandboxing, so it
# is gated by deacon's workspace-trust check.

echo "== Scenario 1: untrusted (DEACON_NO_PROMPT=1) denies host initializeCommand ==" >&2
set +e
DEACON_NO_PROMPT=1 "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null 2>"${SCRIPT_DIR}/deny.log"
status=$?
set -e
if [ "$status" -eq 0 ]; then
	echo "FAIL: up unexpectedly succeeded under DEACON_NO_PROMPT=1" >&2
	rm -f "${SCRIPT_DIR}/deny.log"; exit 1
fi
if [ -f "$MARKER" ]; then
	echo "FAIL: initializeCommand ran despite being untrusted" >&2
	rm -f "${SCRIPT_DIR}/deny.log"; exit 1
fi
if ! grep -qiE "trust|untrusted" "${SCRIPT_DIR}/deny.log"; then
	echo "FAIL: error did not mention workspace trust" >&2
	cat "${SCRIPT_DIR}/deny.log" >&2; rm -f "${SCRIPT_DIR}/deny.log"; exit 1
fi
rm -f "${SCRIPT_DIR}/deny.log"
echo "  ok: denied, initializeCommand did not run" >&2

echo "== Scenario 2: --trust-workspace allows host initializeCommand ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --trust-workspace "$@" >/dev/null
[ -f "$MARKER" ] || { echo "FAIL: initializeCommand did not run under --trust-workspace" >&2; exit 1; }
echo "  ok: trusted, initializeCommand ran" >&2

echo "All scenarios passed." >&2
