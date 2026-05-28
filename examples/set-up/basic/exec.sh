#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 (or python) is required to parse JSON" >&2
		exit 1
	fi
fi

run() {
	echo "+ $*" >&2
	"$@"
}

CID=""
cleanup() {
	if [ -n "${CID:-}" ]; then
		docker rm -f "$CID" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Start a vanilla container outside deacon ==" >&2
CID="$(docker run -d alpine:3.18 sleep infinity)"
echo "Container: $CID" >&2

# Confirm the container has no lifecycle markers yet.
if docker exec "$CID" test -f /tmp/onCreate.flag; then
	echo "FAIL: onCreate marker present before set-up" >&2
	exit 1
fi

echo "== set-up: layer devcontainer.json + run lifecycle ==" >&2
# `set-up` doesn't accept up-style flags; don't forward `$@`. Capture
# stderr so failures are not silent.
SETUP_ERR="$(mktemp)"
snapshot="$(run "$DEACON_BIN" set-up \
	--container-id "$CID" \
	--config "$SCRIPT_DIR/devcontainer.json" \
	--log-format json 2> "$SETUP_ERR")" || {
	echo "FAIL: set-up exited non-zero" >&2
	sed 's/^/  | /' "$SETUP_ERR" >&2
	rm -f "$SETUP_ERR"
	exit 1
}
rm -f "$SETUP_ERR"

# Lifecycle hooks should have fired inside the existing container.
for marker in onCreate postCreate postStart; do
	if ! docker exec "$CID" test -f "/tmp/${marker}.flag"; then
		echo "FAIL: /tmp/${marker}.flag missing after set-up" >&2
		exit 1
	fi
	echo "  ok: /tmp/${marker}.flag present" >&2
done

echo "== Inspect snapshot fields ==" >&2
remote_user="$(printf '%s' "$snapshot" | "$PYTHON_BIN" -c '
import json, sys
print(json.load(sys.stdin).get("configuration", {}).get("remoteUser", ""))
')"
remote_env_demo="$(printf '%s' "$snapshot" | "$PYTHON_BIN" -c '
import json, sys
env = json.load(sys.stdin).get("configuration", {}).get("remoteEnv", {}) or {}
print(env.get("DEACON_SET_UP_DEMO", ""))
')"
echo "  remoteUser=${remote_user}" >&2
echo "  DEACON_SET_UP_DEMO=${remote_env_demo}" >&2
[ "$remote_user" = "root" ] || { echo "FAIL: remoteUser != root" >&2; exit 1; }
[ "$remote_env_demo" = "1" ] || { echo "FAIL: DEACON_SET_UP_DEMO != 1" >&2; exit 1; }

echo "== set-up --skip-post-create: snapshot only ==" >&2
# Start a fresh vanilla container so lifecycle hasn't run, then prove it stays absent.
docker rm -f "$CID" >/dev/null 2>&1 || true
CID="$(docker run -d alpine:3.18 sleep infinity)"
run "$DEACON_BIN" set-up \
	--container-id "$CID" \
	--config "$SCRIPT_DIR/devcontainer.json" \
	--skip-post-create >/dev/null
if docker exec "$CID" test -f /tmp/onCreate.flag; then
	echo "FAIL: lifecycle ran despite --skip-post-create" >&2
	exit 1
fi
echo "  ok: lifecycle suppressed" >&2

echo "All scenarios passed." >&2
