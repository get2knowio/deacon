#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

run() {
	echo "+ $*" >&2
	"$@"
}

cd "$SCRIPT_DIR"

# `outdated` resolves the configured Features against their registry and reports
# current | wanted | latest. It needs network to ghcr.io but no Docker daemon.
# The config pins git to an old version so it is reported as outdated.

echo "== Scenario 1: outdated --output json emits a report ==" >&2
out="$(run "$DEACON_BIN" outdated --workspace-folder "$SCRIPT_DIR" --output json 2>/dev/null)"
echo "$out" | sed 's/^/  | /' | head -n 20 >&2
echo "$out" | grep -q 'git' || { echo "FAIL: git feature missing from outdated report" >&2; exit 1; }

if command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	printf '%s' "$out" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); assert isinstance(d, dict) and d, "empty report"' \
		|| { echo "FAIL: outdated --output json was not a non-empty JSON object" >&2; exit 1; }
fi
echo "  ok: JSON report includes the git feature" >&2

echo "== Scenario 2: --fail-on-outdated gates CI with a non-zero exit ==" >&2
set +e
run "$DEACON_BIN" outdated --workspace-folder "$SCRIPT_DIR" --output json --fail-on-outdated >/dev/null 2>&1
code=$?
set -e
echo "  exit code: ${code}" >&2
# git:1.0.0 should be behind latest, so --fail-on-outdated must signal non-zero.
[ "$code" -ne 0 ] || { echo "FAIL: --fail-on-outdated returned 0 despite an outdated feature" >&2; exit 1; }
echo "  ok: non-zero exit when outdated" >&2

echo "All scenarios passed." >&2
