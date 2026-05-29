#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
LOCKFILE="${SCRIPT_DIR}/.devcontainer/devcontainer-lock.json"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature lockfile lifecycle" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	rm -f "$LOCKFILE"
}
trap cleanup EXIT

cd "$SCRIPT_DIR"
rm -f "$LOCKFILE"

# Scenario 1: a plain `up` resolves the OCI feature and writes a lockfile that
# pins it to a content digest.
echo "== Scenario 1: up generates devcontainer-lock.json ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
[ -f "$LOCKFILE" ] || { echo "FAIL: lockfile not generated at ${LOCKFILE}" >&2; exit 1; }
grep -q 'sha256:' "$LOCKFILE" || { echo "FAIL: lockfile has no pinned digest" >&2; exit 1; }
echo "  ok: lockfile written with a digest" >&2

# Scenario 2: --frozen-lockfile succeeds when the lockfile already matches.
echo "== Scenario 2: up --frozen-lockfile passes when lock matches ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --frozen-lockfile "$@" >/dev/null
echo "  ok: frozen up succeeded against a matching lock" >&2

# Scenario 3: tamper the pinned digest -> --frozen-lockfile must fail closed.
echo "== Scenario 3: --frozen-lockfile fails on a mismatched lock ==" >&2
# Flip the resolved digest to a bogus one.
sed -i 's/sha256:[0-9a-f]\{64\}/sha256:0000000000000000000000000000000000000000000000000000000000000000/' "$LOCKFILE"
set +e
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --frozen-lockfile "$@" >/dev/null 2>"${SCRIPT_DIR}/frozen.log"
code=$?
set -e
echo "  exit code: ${code}" >&2
if [ "$code" -eq 0 ]; then
	echo "FAIL: --frozen-lockfile accepted a tampered lockfile" >&2
	rm -f "${SCRIPT_DIR}/frozen.log"; exit 1
fi
rm -f "${SCRIPT_DIR}/frozen.log"
echo "  ok: frozen up rejected the mismatched lock" >&2

echo "All scenarios passed." >&2
