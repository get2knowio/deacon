#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature env-var injection (_REMOTE_USER et al)" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

chmod +x "$SCRIPT_DIR"/capture-env/install.sh

cd "$SCRIPT_DIR"

echo "== Bring container up (install feature) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Read captured snapshot ==" >&2
docker exec "$cid" cat /usr/local/share/feature-env/snapshot | sed 's/^/  | /' >&2

echo "== Scenario 1: every required var was set ==" >&2
for var in _REMOTE_USER _REMOTE_USER_HOME _CONTAINER_USER _CONTAINER_USER_HOME; do
	if docker exec "$cid" grep -q "^${var}=<unset>$" /usr/local/share/feature-env/snapshot; then
		echo "FAIL: ${var} was <unset> during install.sh" >&2
		exit 1
	fi
	echo "  ok: ${var} set" >&2
done

echo "== Scenario 2: values match resolved config ==" >&2
assert_kv() {
	local key="$1" expected="$2"
	local actual
	actual="$(docker exec "$cid" sh -c "grep ^${key}= /usr/local/share/feature-env/snapshot | cut -d= -f2-" | tr -d '\n')"
	if [ "$actual" != "$expected" ]; then
		echo "FAIL: ${key} expected '${expected}', got '${actual}'" >&2
		exit 1
	fi
	echo "  ok: ${key}=${actual}" >&2
}
assert_kv _REMOTE_USER vscode
assert_kv _REMOTE_USER_HOME /home/vscode
assert_kv _CONTAINER_USER vscode
assert_kv _CONTAINER_USER_HOME /home/vscode

echo "All scenarios passed." >&2
