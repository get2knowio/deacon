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

check_shape() {
	local name="$1" cfg="$2" expected_type="$3"
	echo "== Scenario: ${name} (${cfg}) ==" >&2
	local tmp
	tmp="$(mktemp)"
	run "$DEACON_BIN" read-configuration --config "$SCRIPT_DIR/$cfg" >"$tmp" 2>/dev/null
	"$PYTHON_BIN" "$SCRIPT_DIR/_assert_gpu.py" "$expected_type" "$tmp"
	rm -f "$tmp"

	echo "-- doctor against ${cfg} --" >&2
	set +e
	doctor_out="$("$DEACON_BIN" doctor --config "$SCRIPT_DIR/$cfg" --json 2>/dev/null)"
	doctor_status=$?
	set -e
	if [ -z "${doctor_out:-}" ]; then
		echo "  note: doctor produced no JSON output for ${cfg}" >&2
	else
		if echo "$doctor_out" | "$PYTHON_BIN" -m json.tool >/dev/null 2>&1; then
			echo "  ok: doctor emitted valid JSON (exit ${doctor_status})" >&2
		else
			echo "  note: doctor output not valid JSON" >&2
		fi
	fi
}

check_shape "GPU required"      gpu-true.json     bool
check_shape "GPU optional"      gpu-optional.json string
check_shape "GPU constrained"   gpu-object.json   object

echo "All scenarios run." >&2
