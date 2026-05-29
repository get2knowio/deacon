#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
BUNDLE="${SCRIPT_DIR}/support-bundle"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	rm -rf "$BUNDLE" "${BUNDLE}.zip" "${BUNDLE}.tar.gz"
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

# `doctor` collects environment diagnostics; it needs no devcontainer config.
echo "== Scenario 1: text diagnostics run ==" >&2
out="$(run "$DEACON_BIN" doctor 2>/dev/null || true)"
[ -n "$out" ] || { echo "FAIL: doctor produced no output" >&2; exit 1; }
echo "$out" | sed 's/^/  | /' | head -n 20 >&2
echo "  ok: doctor emitted diagnostics" >&2

echo "== Scenario 2: --json emits parseable JSON ==" >&2
json="$(run "$DEACON_BIN" doctor --json 2>/dev/null)"
if command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	printf '%s' "$json" | "$PYTHON_BIN" -c 'import json,sys; json.load(sys.stdin)' \
		|| { echo "FAIL: doctor --json did not emit valid JSON" >&2; exit 1; }
	echo "  ok: valid JSON" >&2
else
	printf '%s' "$json" | grep -q '{' || { echo "FAIL: no JSON object" >&2; exit 1; }
	echo "  ok: JSON-ish output (no python to fully validate)" >&2
fi

echo "== Scenario 3: --bundle writes a support bundle ==" >&2
rm -rf "$BUNDLE" "${BUNDLE}.zip" "${BUNDLE}.tar.gz"
run "$DEACON_BIN" doctor --bundle "$BUNDLE" >/dev/null 2>&1
# Accept a directory or an archive artifact at/near the requested path.
if [ -e "$BUNDLE" ] || [ -e "${BUNDLE}.zip" ] || [ -e "${BUNDLE}.tar.gz" ] || ls "${BUNDLE}"* >/dev/null 2>&1; then
	echo "  ok: support bundle created" >&2
else
	echo "FAIL: no support bundle artifact at ${BUNDLE}*" >&2
	exit 1
fi

echo "All scenarios passed." >&2
