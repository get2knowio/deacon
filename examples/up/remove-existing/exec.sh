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
# - "Normal Reconnection (Default Behavior)" -> first two runs
# - "Force Container Replacement"            -> run with --remove-existing-container
# - "Verify Replacement"                     -> compare IDs/timestamps
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

deacon_up() {
	"$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" "$@"
}

cd "$SCRIPT_DIR"

echo "Creating container (first run)..." >&2
output1="$(deacon_up "$@")"
container1="$(extract_container_id "$output1")"
echo "Container created: ${container1}" >&2
timestamp1="$(docker exec "$container1" cat /tmp/created.txt)"
echo "Creation marker:" >&2
echo "$timestamp1" >&2

# Second run to demonstrate default reconnection behavior.
echo "Reconnecting without removal (should reuse container)..." >&2
output2="$(deacon_up "$@")"
container2="$(extract_container_id "$output2")"
echo "Reconnected container: ${container2}" >&2

if [ "$container1" = "$container2" ]; then
	echo "✓ Reused existing container" >&2
else
	echo "✗ Expected reuse but got a new container" >&2
fi

sleep 2

# Force replacement with README flag.
echo "Forcing recreation with --remove-existing-container..." >&2
output3="$(deacon_up --remove-existing-container "$@")"
container3="$(extract_container_id "$output3")"
timestamp3="$(docker exec "$container3" cat /tmp/created.txt)"

echo "New container: ${container3}" >&2
echo "New creation marker:" >&2
echo "$timestamp3" >&2

if [ "$container3" != "$container1" ]; then
	echo "✓ Container was replaced after removal" >&2
else
	echo "✗ Container ID did not change after removal" >&2
fi

# Cleanup containers.
for cid in "$container1" "$container2" "$container3"; do
	if [ -n "$cid" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
done
