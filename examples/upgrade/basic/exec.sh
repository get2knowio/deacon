#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
LOCKFILE="${SCRIPT_DIR}/.devcontainer/devcontainer-lock.json"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	rm -f "$LOCKFILE"
}
trap cleanup EXIT

cd "$SCRIPT_DIR"
rm -f "$LOCKFILE"

# `upgrade` re-resolves the configured Features and (re)writes the lockfile.
# It needs network to ghcr.io but no Docker daemon. `--dry-run` prints the
# lockfile JSON to stdout instead of writing it.

echo "== Scenario 1: upgrade --dry-run prints a resolved lockfile ==" >&2
out="$(run "$DEACON_BIN" upgrade --workspace-folder "$SCRIPT_DIR" --dry-run 2>/dev/null)"
echo "$out" | sed 's/^/  | /' | head -n 25 >&2
echo "$out" | grep -q 'git' || { echo "FAIL: git feature missing from lockfile output" >&2; exit 1; }
echo "$out" | grep -q 'sha256:' || { echo "FAIL: no resolved digest (sha256:) in lockfile output" >&2; exit 1; }
if command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	printf '%s' "$out" | "$PYTHON_BIN" -c 'import json,sys; json.load(sys.stdin)' \
		|| { echo "FAIL: --dry-run output was not valid JSON" >&2; exit 1; }
fi
echo "  ok: dry-run produced a resolved, digest-pinned lockfile" >&2
[ -f "$LOCKFILE" ] && { echo "FAIL: --dry-run must not write the lockfile to disk" >&2; exit 1; }
echo "  ok: --dry-run did not touch disk" >&2

echo "== Scenario 2: upgrade (no --dry-run) writes devcontainer-lock.json ==" >&2
run "$DEACON_BIN" upgrade --workspace-folder "$SCRIPT_DIR" >/dev/null 2>&1
[ -f "$LOCKFILE" ] || { echo "FAIL: lockfile not written at ${LOCKFILE}" >&2; exit 1; }
grep -q 'sha256:' "$LOCKFILE" || { echo "FAIL: written lockfile has no digest" >&2; exit 1; }
echo "  ok: lockfile written with a pinned digest" >&2

echo "All scenarios passed." >&2
