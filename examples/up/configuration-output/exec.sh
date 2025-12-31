#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 (or python) is required to parse JSON output" >&2
		exit 1
	fi
fi

# README sections: "Default Output" vs "Include Base Configuration" vs "Include Merged Configuration" vs "Include Both".
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" - <<'PY'
import json, sys
data = json.load(sys.stdin)
print(data.get("containerId", ""))
PY
}

cleanup_container() {
	if [ -n "$1" ]; then
		docker rm -f "$1" >/dev/null 2>&1 || true
	fi
}

cd "$SCRIPT_DIR"

echo "== Default Output (no configuration flags) ==" >&2
out_default="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
cleanup_container "$(extract_container_id "$out_default")"

echo "== Include Base Configuration ==" >&2
out_base="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--include-configuration \
	"$@")"
cleanup_container "$(extract_container_id "$out_base")"

echo "== Include Merged Configuration ==" >&2
out_merged="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--include-merged-configuration \
	"$@")"
cleanup_container "$(extract_container_id "$out_merged")"

echo "== Include Both Configurations ==" >&2
out_both="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--include-configuration \
	--include-merged-configuration \
	"$@")"
cleanup_container "$(extract_container_id "$out_both")"
