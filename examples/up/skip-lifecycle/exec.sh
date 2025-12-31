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

# README mapping:
# - "Normal Up (All Lifecycle Commands)" -> first run
# - "Skip Post-Create Lifecycle"        -> second run with --skip-post-create
# - "Skip Only Post-Attach"             -> third run
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

echo "Running with full lifecycle..." >&2
full_output="$( run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" )"
full_container="$(extract_container_id "$full_output")"
echo "Lifecycle executed in container: ${full_container}" >&2

# Matches README "Skip Post-Create" section.
echo "Running with --skip-post-create (skips all lifecycle hooks)..." >&2
skip_output="$( run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --skip-post-create "$@" )"
skip_container="$(extract_container_id "$skip_output")"
echo "Lifecycle skipped container: ${skip_container}" >&2

# Matches README "Skip Only Post-Attach".
echo "Running with --skip-post-attach..." >&2
attach_output="$( run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --skip-post-attach "$@" )"
attach_container="$(extract_container_id "$attach_output")"
echo "Skipped post-attach in container: ${attach_container}" >&2

# Cleanup containers from each run.
for cid in "$full_container" "$skip_container" "$attach_container"; do
	if [ -n "$cid" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
done
