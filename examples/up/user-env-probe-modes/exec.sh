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

# Runtime artifacts `deacon up` / `deacon build` may write into this example's
# workspace. Removing only these generated paths leaves the directory exactly as
# committed (#179); the committed `.devcontainer/` config is never touched.
clean_workspace_artifacts() {
	rm -rf \
		"${SCRIPT_DIR}/.devcontainer-state" \
		"${SCRIPT_DIR}/.devcontainer/build-cache" \
		"${SCRIPT_DIR}/.deacon" \
		"${SCRIPT_DIR}/.deacon-temp-build"
	# The lockfile sits beside the config and gains a leading dot when the
	# config basename has one (`.devcontainer.json` -> `.devcontainer-lock.json`).
	rm -f \
		"${SCRIPT_DIR}/devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer/devcontainer-lock.json" \
		"${SCRIPT_DIR}/.devcontainer/.devcontainer-lock.json"
}
trap 'cleanup; clean_workspace_artifacts' EXIT

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
