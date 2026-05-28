#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
VOLUME_NAME="deacon-down-demo"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Down: basic shutdown" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	docker volume rm "$VOLUME_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

# Extra args (e.g. --mount-workspace-git-root false) are forwarded to
# `up` only — `down` doesn't accept the same set.
UP_ARGS=( "$@" )

up() {
	run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "${UP_ARGS[@]}" >/dev/null
}

assert_container_state() {
	local expected="$1" cid status
	cid="$(container_id || true)"
	if [ -z "${cid:-}" ]; then
		status="absent"
	else
		status="$(docker inspect -f '{{.State.Status}}' "$cid" 2>/dev/null || echo absent)"
	fi
	if [ "$status" != "$expected" ]; then
		echo "FAIL: container expected ${expected}, got ${status}" >&2
		exit 1
	fi
	echo "  ok: container ${status}" >&2
}

# Scenario 1: default stop (stopContainer, no --remove).
echo "== Scenario 1: up then down (stop only) ==" >&2
up
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR"
assert_container_state exited

# Scenario 2: --remove deletes the container.
echo "== Scenario 2: down --remove ==" >&2
up
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --remove
assert_container_state absent

# Scenario 3: --remove --volumes also drops anonymous volumes.
echo "== Scenario 3: down --remove --volumes ==" >&2
up
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --remove --volumes
assert_container_state absent

# Scenario 4: --force teardown.
echo "== Scenario 4: down --force --remove ==" >&2
up
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --force --remove
assert_container_state absent

# Scenario 5: --all sweeps stale containers matching the workspace labels.
echo "== Scenario 5: down --all --remove (idempotent across stale) ==" >&2
up
# Force a second stale container with the same workspace label.
docker run -d --rm \
	--label "devcontainer.local_folder=${SCRIPT_DIR}" \
	alpine:3.18 sleep infinity >/dev/null
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --all --remove
stragglers="$(docker ps -a --filter "label=devcontainer.local_folder=${SCRIPT_DIR}" -q | wc -l)"
[ "$stragglers" -eq 0 ] || { echo "FAIL: ${stragglers} stale containers remain" >&2; exit 1; }
echo "  ok: no stale containers remain" >&2

# Scenario 6: down on nothing is a no-op.
echo "== Scenario 6: idempotent down ==" >&2
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --remove
echo "  ok: down on absent container exited 0" >&2

echo "All scenarios passed." >&2
