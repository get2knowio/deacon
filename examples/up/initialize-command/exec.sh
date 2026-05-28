#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

TMP_ROOT="$(mktemp -d)"
USER_DATA_FOLDER="${TMP_ROOT}/user-data"
mkdir -p "$USER_DATA_FOLDER"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=initializeCommand + workspace-trust gate" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	rm -rf "$TMP_ROOT"
	rm -f "$SCRIPT_DIR/.initialize-marker"
}
trap cleanup EXIT

clear_marker() { rm -f "$SCRIPT_DIR/.initialize-marker"; }
marker_exists() { [ -f "$SCRIPT_DIR/.initialize-marker" ]; }

assert_marker_present() {
	if ! marker_exists; then
		echo "FAIL: .initialize-marker missing (initializeCommand did not run)" >&2
		exit 1
	fi
	echo "  ok: initializeCommand ran" >&2
}
assert_marker_absent() {
	if marker_exists; then
		echo "FAIL: .initialize-marker present (initializeCommand should have been gated)" >&2
		exit 1
	fi
	echo "  ok: initializeCommand gated" >&2
}

remove_container() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}

cd "$SCRIPT_DIR"

# Scenario 1: default deny — workspace not in store, no flag, no DEACON_NO_PROMPT.
echo "== Scenario 1: default deny (untrusted workspace) ==" >&2
clear_marker
set +e
"$DEACON_BIN" up \
	--workspace-folder "$SCRIPT_DIR" \
	--user-data-folder "$USER_DATA_FOLDER" \
	--remove-existing-container \
	"$@" >/dev/null 2> "$TMP_ROOT/scenario1.stderr"
status=$?
set -e
[ $status -ne 0 ] || { echo "FAIL: expected non-zero exit on untrusted workspace" >&2; exit 1; }
grep -q "WorkspaceUntrusted\|not trusted" "$TMP_ROOT/scenario1.stderr" \
	|| { echo "FAIL: expected WorkspaceUntrusted in stderr"; sed -n '1,40p' "$TMP_ROOT/scenario1.stderr" >&2; exit 1; }
assert_marker_absent

# Scenario 2: --trust-workspace (one-shot). Should pass; store unchanged.
echo "== Scenario 2: --trust-workspace (one-shot allow) ==" >&2
clear_marker
remove_container
run "$DEACON_BIN" up \
	--workspace-folder "$SCRIPT_DIR" \
	--user-data-folder "$USER_DATA_FOLDER" \
	--trust-workspace \
	--remove-existing-container \
	"$@" >/dev/null
assert_marker_present
if [ -f "$USER_DATA_FOLDER/trusted_workspaces.json" ]; then
	if grep -q "$SCRIPT_DIR" "$USER_DATA_FOLDER/trusted_workspaces.json"; then
		echo "FAIL: --trust-workspace should NOT persist to store" >&2
		exit 1
	fi
fi
echo "  ok: trust store unchanged" >&2

# Scenario 3: --trust-workspace-persist — writes to store.
echo "== Scenario 3: --trust-workspace-persist (writes store) ==" >&2
clear_marker
remove_container
run "$DEACON_BIN" up \
	--workspace-folder "$SCRIPT_DIR" \
	--user-data-folder "$USER_DATA_FOLDER" \
	--trust-workspace-persist \
	--remove-existing-container \
	"$@" >/dev/null
assert_marker_present
[ -f "$USER_DATA_FOLDER/trusted_workspaces.json" ] \
	|| { echo "FAIL: trusted_workspaces.json was not created" >&2; exit 1; }
grep -q "$SCRIPT_DIR" "$USER_DATA_FOLDER/trusted_workspaces.json" \
	|| { echo "FAIL: workspace path missing from store" >&2; exit 1; }
echo "  ok: store contains workspace" >&2

# Scenario 4: re-run with no flag — passes from the store.
echo "== Scenario 4: re-run from persisted trust (no flag) ==" >&2
clear_marker
remove_container
run "$DEACON_BIN" up \
	--workspace-folder "$SCRIPT_DIR" \
	--user-data-folder "$USER_DATA_FOLDER" \
	--remove-existing-container \
	"$@" >/dev/null
assert_marker_present

# Scenario 5: DEACON_NO_PROMPT=1 against a fresh user-data-folder fails closed.
echo "== Scenario 5: DEACON_NO_PROMPT=1 (CI fail-closed) ==" >&2
clear_marker
remove_container
FRESH_UDF="$TMP_ROOT/user-data-ci"
mkdir -p "$FRESH_UDF"
set +e
DEACON_NO_PROMPT=1 "$DEACON_BIN" up \
	--workspace-folder "$SCRIPT_DIR" \
	--user-data-folder "$FRESH_UDF" \
	--remove-existing-container \
	"$@" >/dev/null 2> "$TMP_ROOT/scenario5.stderr"
status=$?
set -e
[ $status -ne 0 ] || { echo "FAIL: DEACON_NO_PROMPT=1 should fail closed" >&2; exit 1; }
assert_marker_absent
echo "  ok: CI fail-closed honored" >&2

echo "All scenarios passed." >&2
