#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps --filter "label=devcontainer.local_folder=${SCRIPT_DIR}" --format "{{.ID}}" | head -n1
}

cleanup() {
	local cid
	cid="$(container_id)"
	if [ -n "$cid" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	rm -f "${SCRIPT_DIR}/output.txt" "${SCRIPT_DIR}/errors.txt" "${SCRIPT_DIR}/build.log"
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Start dev container ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@"

echo "== Separate stdout/stderr to files (README: Separate Streams) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/generate-output.sh \
	> "${SCRIPT_DIR}/output.txt" 2> "${SCRIPT_DIR}/errors.txt"
echo "--- stdout ---" >&2
cat "${SCRIPT_DIR}/output.txt"
echo "--- stderr ---" >&2
cat "${SCRIPT_DIR}/errors.txt"

echo "== Pipe stdout to another command (README: Pipe stdout) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/generate-output.sh \
	| grep "stdout line" | head -n 3

echo "== Binary-safe operation (README: Binary-Safe Operation) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" cat /workspace/data/sample.bin \
	| xxd | head -n 5

echo "== Process substitution (README: Process Substitution) ==" >&2
cat <(run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/generate-output.sh)

echo "== CI/CD pattern (README: CI/CD Pattern) ==" >&2
set +e
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash /workspace/generate-output.sh > "${SCRIPT_DIR}/build.log" 2>&1
EXIT_CODE=$?
set -e
echo "Build exit code: $EXIT_CODE" >&2

echo "== JSON output streaming (README: JSON Output) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" --log-format json \
	bash /workspace/generate-output.sh | head -n 5

echo "== Large output streaming (README: Large Output Streaming) ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" bash -c 'for i in {1..50}; do echo "Line $i"; done' | wc -l
