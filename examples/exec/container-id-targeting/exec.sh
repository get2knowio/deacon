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
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Start dev container (README: step 1) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container --mount-workspace-git-root false "$@"

CID="$(container_id)"
if [ -z "$CID" ]; then
	echo "Container not found after up" >&2
	exit 1
fi
echo "Using container: ${CID}" >&2

echo "== Execute simple command via --container-id (README: step 3) ==" >&2
run "$DEACON_BIN" exec --container-id "$CID" echo "Hello from container"

echo "== Run test script via --container-id (README: step 4) ==" >&2
run "$DEACON_BIN" exec --container-id "$CID" bash /workspace/test-script.sh

echo "== Check environment variables (README: step 5) ==" >&2
# `--container-id` on its own is a low-level escape hatch: it names a container,
# not a workspace, so deacon loads NO devcontainer.json and applies none of its
# config — including `remoteEnv`. The container's own environment is what you
# get. Assert that positively rather than grepping for a `remoteEnv` key that
# this targeting mode does not promise.
container_env="$(run "$DEACON_BIN" exec --container-id "$CID" env)"
if ! printf '%s' "$container_env" | grep -q '^PATH='; then
	echo "FAIL: expected the container's own environment via --container-id" >&2
	exit 1
fi
if printf '%s' "$container_env" | grep -q '^CONTAINER_ENV_VAR='; then
	echo "FAIL: --container-id applied config remoteEnv; see the note below." >&2
	exit 1
fi
echo "  ok: container env present, config remoteEnv correctly NOT applied" >&2

echo "== remoteEnv DOES apply when the config is named (README: step 6) ==" >&2
# Naming the workspace gives deacon a config to resolve, so `remoteEnv` applies.
# This is the contrast that makes the targeting modes legible.
workspace_env="$(run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" \
	--mount-workspace-git-root false env)"
if ! printf '%s' "$workspace_env" | grep -q '^CONTAINER_ENV_VAR=set-by-config'; then
	echo "FAIL: remoteEnv not applied on the --workspace-folder path" >&2
	exit 1
fi
echo "  ok: remoteEnv applied when the workspace names the config" >&2
