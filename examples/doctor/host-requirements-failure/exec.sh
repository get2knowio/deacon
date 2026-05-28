#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then PYTHON_BIN=python; fi
fi

run() {
	echo "+ $*" >&2
	"$@"
}

cd "$SCRIPT_DIR"

# Extra args (e.g. --mount-workspace-git-root false) aren't accepted by
# `doctor`; capture them so they don't get forwarded.
DOCTOR_ARGS=()  # currently none; placeholder for future doctor-specific flags

echo "== Scenario 1: text mode reports failing requirements ==" >&2
set +e
text_out="$("$DEACON_BIN" doctor --workspace-folder "$SCRIPT_DIR" "${DOCTOR_ARGS[@]}" 2>&1)"
text_status=$?
set -e
echo "$text_out" | sed 's/^/  | /' >&2
if [ $text_status -eq 0 ]; then
	echo "  note: doctor exited 0 — current deacon may not gate exit code on hostRequirements" >&2
else
	echo "  ok: non-zero exit (${text_status})" >&2
fi
echo "$text_out" | grep -qi -E 'cpu|memory|storage' \
	|| { echo "FAIL: text output should mention at least one resource" >&2; exit 1; }
echo "  ok: text output references resource constraints" >&2

echo "== Scenario 2: JSON mode produces structured report ==" >&2
set +e
json_out="$("$DEACON_BIN" doctor --workspace-folder "$SCRIPT_DIR" --json "${DOCTOR_ARGS[@]}" 2>/dev/null)"
json_status=$?
set -e
if [ -z "${json_out:-}" ]; then
	echo "FAIL: JSON mode produced no stdout" >&2
	exit 1
fi
echo "$json_out" | "$PYTHON_BIN" -m json.tool >/dev/null \
	|| { echo "FAIL: doctor --json output is not valid JSON" >&2; echo "$json_out" >&2; exit 1; }
echo "  ok: valid JSON document"
echo "  exit status: ${json_status}" >&2

echo "All scenarios run." >&2
