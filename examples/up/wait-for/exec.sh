#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Up waitFor (default updateContentCommand)" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}

# Runtime artifacts `deacon up` / `deacon build` may write into this example's
# workspace. Removing only these generated paths leaves the directory exactly as
# committed (#179); the committed `.devcontainer/` config is never touched.
clean_workspace_artifacts() {
	rm -rf \
		"${SCRIPT_DIR}/.devcontainer-state" \
		"${SCRIPT_DIR}/.devcontainer/build-cache" \
		"${SCRIPT_DIR}/.deacon" \
		"${SCRIPT_DIR}/.deacon-temp-build"
	# The lockfile sits beside the config and gains a leading dot when the
	# config basename has one (`.devcontainer.json` -> `.devcontainer-lock.json`).
	rm -f \
		"${SCRIPT_DIR}/devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer/devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer/.devcontainer-lock.json"
}
trap 'cleanup; clean_workspace_artifacts' EXIT

read_log() {
	local cid
	cid="$(container_id)"
	docker exec "$cid" cat /tmp/lifecycle.log 2>/dev/null || true
}

assert_log_contains() {
	local needle="$1"
	read_log | grep -q "^${needle}$" || { echo "FAIL: expected '${needle}' in lifecycle.log" >&2; read_log >&2; exit 1; }
	echo "  ok: log contains ${needle}" >&2
}
assert_log_missing() {
	local needle="$1"
	if read_log | grep -q "^${needle}$"; then
		echo "FAIL: '${needle}' should NOT be in lifecycle.log" >&2
		read_log >&2
		exit 1
	fi
	echo "  ok: log missing ${needle}" >&2
}

cd "$SCRIPT_DIR"

echo "== Scenario 1: default waitFor=updateContentCommand + --skip-non-blocking-commands ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--skip-non-blocking-commands "$@" >/dev/null
sleep 1
assert_log_contains onCreate
assert_log_contains updateContent
assert_log_missing postStart
assert_log_missing postAttach

echo "== Scenario 2: waitFor=onCreateCommand + --skip-non-blocking-commands ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--merge-config ./override.onCreate.json \
	--skip-non-blocking-commands "$@" >/dev/null
sleep 1
assert_log_contains onCreate
assert_log_missing updateContent
assert_log_missing postCreate

echo "== Scenario 3: waitFor=postCreateCommand + --skip-non-blocking-commands ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--merge-config ./override.postCreate.json \
	--skip-non-blocking-commands "$@" >/dev/null
sleep 1
assert_log_contains onCreate
assert_log_contains updateContent
assert_log_contains postCreate
assert_log_missing postStart
assert_log_missing postAttach

echo "== Scenario 4: default waitFor without skip flag (all phases) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
sleep 1
assert_log_contains onCreate
assert_log_contains updateContent
assert_log_contains postCreate
assert_log_contains postStart
assert_log_contains postAttach

echo "All scenarios passed." >&2
