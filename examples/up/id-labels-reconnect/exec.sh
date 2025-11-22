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
# - "Create Container with ID Labels"   -> first run
# - "Reconnect to Existing Container"   -> second run with same labels
# - "Expect Existing Container"         -> final run with --expect-existing-container
# The label set mirrors the sample in the README.
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

labels=(
	"project=myapp"
	"environment=dev"
	"team=backend"
)

deacon_with_labels() {
	local extra_args=("$@")
	local args=("$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR")
	for label in "${labels[@]}"; do
		args+=("--id-label" "$label")
	done
	args+=("${extra_args[@]}")
	run "${args[@]}"
}

cd "$SCRIPT_DIR"

# Create a labeled container (README: "Create Container with ID Labels").
echo "Creating labeled container..." >&2
output1="$(deacon_with_labels "$@")"
container1="$(extract_container_id "$output1")"
echo "Container created: ${container1}" >&2

# Reconnect using the same labels (README: "Reconnect to Existing Container").
echo "Reconnecting with the same labels..." >&2
output2="$(deacon_with_labels "$@")"
container2="$(extract_container_id "$output2")"

if [ "$container1" = "$container2" ]; then
	echo "✓ Reconnected to existing container ${container2}" >&2
else
	echo "✗ Expected to reconnect but got a new container (${container2})" >&2
fi

# Fail if missing (README: "Expect Existing Container").
echo "Expecting existing container only..." >&2
deacon_with_labels --expect-existing-container "$@"

# Cleanup created container.
if [ -n "$container1" ]; then
	docker rm -f "$container1" >/dev/null 2>&1 || true
fi
