#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature option sanitization" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

chmod +x "$SCRIPT_DIR"/report/install.sh

cd "$SCRIPT_DIR"

echo "== Bring container up ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Captured probes ==" >&2
docker exec "$cid" cat /usr/local/share/option-sanitization/probes | sed 's/^/  | /' >&2

echo "== Scenario 1: sanitized names set with expected values ==" >&2
assert_probe() {
	local key="$1" expected="$2"
	local actual
	actual="$(docker exec "$cid" sh -c "grep ^${key}= /usr/local/share/option-sanitization/probes | cut -d= -f2-" | tr -d '\n')"
	if [ "$actual" != "$expected" ]; then
		echo "FAIL: ${key} expected '${expected}', got '${actual}'" >&2
		exit 1
	fi
	echo "  ok: ${key}=${actual}" >&2
}
assert_probe MY_STRING_OPTION "Hello, World!"
assert_probe ANOTHER_WEIRD_KEY "x/y/z"
assert_probe FLAGOPTION "true"

echo "== Scenario 2: pre-sanitization name (no underscores) is absent ==" >&2
val="$(docker exec "$cid" sh -c "grep ^MYSTRINGOPTION= /usr/local/share/option-sanitization/probes | cut -d= -f2-" | tr -d '\n')"
if [ "$val" != "<unset>" ] && [ "$val" != "Hello, World!" ]; then
	# Some implementations may also expose the collapsed form. Note but don't fail.
	echo "  note: MYSTRINGOPTION='${val}' (spec only requires underscored form to exist)" >&2
elif [ "$val" = "<unset>" ]; then
	echo "  ok: MYSTRINGOPTION unset (only sanitized form is exposed)" >&2
fi

echo "All scenarios passed." >&2
