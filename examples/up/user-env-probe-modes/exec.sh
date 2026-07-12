#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Up userEnvProbe (default loginInteractiveShell)" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

run_mode() {
	local label="$1"
	shift
	# Remaining positional args are forwarded to deacon as override flags.
	echo "== userEnvProbe: ${label} ==" >&2
	run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
	local cid
	cid="$(container_id)"
	# Inject PROBE_VAR into ~/.bashrc so interactive probes pick it up,
	# then re-run lifecycle so postCreate captures the freshly probed env.
	docker exec -u vscode "$cid" bash -lc 'grep -q PROBE_VAR ~/.bashrc || echo "export PROBE_VAR=set" >> ~/.bashrc'
	docker exec -u vscode "$cid" sh -c 'rm -f /tmp/probe.path /tmp/probe.var'
	run "$DEACON_BIN" run-user-commands --workspace-folder "$SCRIPT_DIR" "$@" >/dev/null
	echo "--- PATH ($label) ---" >&2
	docker exec -u vscode "$cid" cat /tmp/probe.path || true
	echo "--- PROBE_VAR ($label) ---" >&2
	docker exec -u vscode "$cid" cat /tmp/probe.var || true
}

run_mode loginInteractiveShell
run_mode interactiveShell --merge-config ./override.interactive.json
run_mode loginShell        --merge-config ./override.login.json
run_mode none              --merge-config ./override.none.json

echo "All probe modes exercised." >&2
