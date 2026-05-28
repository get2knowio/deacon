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

# README: GPU Modes — demonstrates all, detect, none modes for GPU handling.
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" -c 'import json, sys; data = json.load(sys.stdin); print(data.get("containerId", ""))'
}

cd "$SCRIPT_DIR"

# README: "Explicit CPU-Only Runs (Mode: none)" — default behavior, no GPU requests
echo "== GPU Mode: none (default) — no GPU requests or warnings ==" >&2
output_none="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --gpu-mode none --remove-existing-container "$@")"
container_id_none="$(extract_container_id "$output_none")"
echo "Container (none): ${container_id_none}" >&2

if [ -n "$container_id_none" ]; then
	docker rm -f "$container_id_none" >/dev/null 2>&1 || true
fi

# README: "Auto-Detect with Safe Fallback (Mode: detect)" — check GPU availability
echo "== GPU Mode: detect — auto-detect with warning on non-GPU hosts ==" >&2
# Stderr goes to a temp log so we can grep for the warning without polluting
# the JSON capture on stdout (per the streams contract in CLAUDE.md).
stderr_log="$(mktemp)"
output_detect="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --gpu-mode detect --remove-existing-container "$@" 2> "$stderr_log")"
container_id_detect="$(extract_container_id "$output_detect")"
echo "Container (detect): ${container_id_detect}" >&2
if grep -q "GPU mode 'detect'" "$stderr_log"; then
	echo "  ok: detect-mode warning emitted on non-GPU host" >&2
fi
rm -f "$stderr_log"

if [ -n "$container_id_detect" ]; then
	docker rm -f "$container_id_detect" >/dev/null 2>&1 || true
fi

# README: "Guarantee GPU Access (Mode: all)" — requires GPU-capable host
# Note: This may fail on non-GPU hosts with a Docker runtime error
echo "== GPU Mode: all — request GPU resources (may fail on non-GPU hosts) ==" >&2
stderr_log="$(mktemp)"
if output_all="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --gpu-mode all --remove-existing-container "$@" 2> "$stderr_log")"; then
	container_id_all="$(extract_container_id "$output_all")"
	echo "Container (all): ${container_id_all}" >&2

	if [ -n "$container_id_all" ]; then
		docker rm -f "$container_id_all" >/dev/null 2>&1 || true
	fi
else
	echo "GPU mode 'all' failed (expected on non-GPU hosts)" >&2
	# This is acceptable behavior — mode 'all' should fail if GPUs unavailable
fi
rm -f "$stderr_log"

echo "== GPU modes example completed successfully ==" >&2
