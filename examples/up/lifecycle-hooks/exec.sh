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

# README: "Normal Up (All Hooks)" — runs onCreate, updateContent, postCreate, postStart, postAttach.
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

cd "$SCRIPT_DIR"
# Execute the full lifecycle path described in the README.
echo "== Lifecycle Hooks: full sequence onCreate -> postAttach ==" >&2
out="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
cid="$(extract_container_id "$out")"
if [ -n "$cid" ]; then
	docker rm -f "$cid" >/dev/null 2>&1 || true
fi
