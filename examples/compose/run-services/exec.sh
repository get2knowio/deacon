#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

svc_running() {
	# 1 if the given compose service is running, else 0.
	docker ps --filter "label=canary.svc=$1" --filter "status=running" -q | grep -q . && echo 1 || echo 0
}

cleanup() {
	docker ps -a --filter "label=canary.group=runsvc" -q | xargs -r docker rm -f >/dev/null 2>&1 || true
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Bring up with service=app, runServices=[worker] ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null

echo "== Scenario: only app + worker start; idle stays down ==" >&2
app="$(svc_running app)"; worker="$(svc_running worker)"; idle="$(svc_running idle)"
echo "  app=${app} worker=${worker} idle=${idle}" >&2
[ "$app" = "1" ]    || { echo "FAIL: primary service 'app' not running" >&2; exit 1; }
[ "$worker" = "1" ] || { echo "FAIL: runServices entry 'worker' not running" >&2; exit 1; }
[ "$idle" = "0" ]   || { echo "FAIL: 'idle' should NOT start (not in service/runServices)" >&2; exit 1; }
echo "  ok: runServices selectivity honored" >&2

echo "All scenarios passed." >&2
