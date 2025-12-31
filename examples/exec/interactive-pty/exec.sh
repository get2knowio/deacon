#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

pty() {
	# Use `script` to allocate a PTY even when this script runs non-interactively.
	run script -q /dev/null -c "$1"
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
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Start dev container (Prerequisite) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@"

echo "== Interactive shell via PTY (README: Interactive Shell) ==" >&2
pty "$DEACON_BIN exec --workspace-folder \"$SCRIPT_DIR\" bash -lc 'echo \"shell user: \$(whoami)\"; tty'"

echo "== Python REPL style (README: Python REPL) ==" >&2
pty "printf 'print(\\\"hello from python\\\")\\nexit()\\n' | $DEACON_BIN exec --workspace-folder \"$SCRIPT_DIR\" python3"

echo "== Interactive script with input (README: Interactive Script with Input) ==" >&2
pty "printf 'Alice\\nyes\\n' | $DEACON_BIN exec --workspace-folder \"$SCRIPT_DIR\" python3 /workspace/interactive-demo.py"

echo "== Node.js REPL style (README: Node.js REPL) ==" >&2
pty "printf 'console.log(process.version)\\n.exit\\n' | $DEACON_BIN exec --workspace-folder \"$SCRIPT_DIR\" node"

echo "== Terminal size control (README: Terminal Size Control) ==" >&2
pty "$DEACON_BIN exec --workspace-folder \"$SCRIPT_DIR\" --terminal-columns 120 --terminal-rows 40 bash /workspace/terminal-test.sh"

echo "== TTY detection (README: TTY Detection) ==" >&2
pty "$DEACON_BIN exec --workspace-folder \"$SCRIPT_DIR\" tty"
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" tty </dev/null || true
