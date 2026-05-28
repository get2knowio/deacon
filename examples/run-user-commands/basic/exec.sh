#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Run User Commands Example" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

assert_marker() {
	local marker="$1" expected_present="$2" cid
	cid="$(container_id)"
	if docker exec "$cid" test -f "/tmp/${marker}.flag" 2>/dev/null; then
		actual="present"
	else
		actual="absent"
	fi
	if [ "$actual" != "$expected_present" ]; then
		echo "FAIL: /tmp/${marker}.flag expected ${expected_present}, got ${actual}" >&2
		exit 1
	fi
	echo "  ok: /tmp/${marker}.flag ${actual}" >&2
}

cd "$SCRIPT_DIR"

# Extra args (e.g. --mount-workspace-git-root false) only apply to `up`;
# run-user-commands doesn't accept the same flag set.
UP_ARGS=( "$@" )

# Scenario 1: bring container up with the post-create phase suppressed.
# `--skip-post-create` only suppresses the postCreate phase onward;
# onCreate and updateContent still fire as part of `up`.
echo "== up --skip-post-create (only post-create+ suppressed) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" \
	--remove-existing-container --skip-post-create "${UP_ARGS[@]}" >/dev/null
assert_marker onCreate present
assert_marker updateContent present
assert_marker postCreate absent
assert_marker postStart absent
assert_marker postAttach absent

# Scenario 2: run-user-commands drives the remaining phases.
echo "== run-user-commands (postCreate+ phases fire) ==" >&2
# Clear earlier markers so the re-run drives all phases cleanly.
cid="$(container_id)"
docker exec "$cid" sh -c 'rm -f /tmp/onCreate.flag /tmp/updateContent.flag /tmp/postCreate.flag /tmp/postStart.flag /tmp/postAttach.flag'
run "$DEACON_BIN" run-user-commands --workspace-folder "$SCRIPT_DIR" >/dev/null
assert_marker onCreate present
assert_marker updateContent present
assert_marker postCreate present
assert_marker postStart present
assert_marker postAttach present

# Scenario 3: prebuild mode stops after updateContent. We clear the later
# markers first so we can prove they are NOT re-created.
echo "== run-user-commands --prebuild (stops after updateContent) ==" >&2
cid="$(container_id)"
docker exec "$cid" sh -c 'rm -f /tmp/postCreate.flag /tmp/postStart.flag /tmp/postAttach.flag'
run "$DEACON_BIN" run-user-commands --workspace-folder "$SCRIPT_DIR" --prebuild >/dev/null
assert_marker updateContent present
assert_marker postCreate absent
assert_marker postStart absent
assert_marker postAttach absent

# Scenario 4: --skip-non-blocking-commands honors waitFor (default updateContent).
echo "== run-user-commands --skip-non-blocking-commands ==" >&2
docker exec "$cid" sh -c 'rm -f /tmp/postStart.flag /tmp/postAttach.flag'
run "$DEACON_BIN" run-user-commands --workspace-folder "$SCRIPT_DIR" \
	--skip-non-blocking-commands >/dev/null
assert_marker postStart absent
assert_marker postAttach absent

# Scenario 5: --container-id targeting (IDE-attach pattern).
echo "== run-user-commands --container-id (no --workspace-folder) ==" >&2
docker exec "$cid" sh -c 'rm -f /tmp/postCreate.flag'
run "$DEACON_BIN" run-user-commands --container-id "$cid" \
	--workspace-folder "$SCRIPT_DIR" >/dev/null
assert_marker postCreate present

echo "All scenarios passed." >&2
