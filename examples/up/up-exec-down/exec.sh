#!/usr/bin/env bash
set -euo pipefail

# Compound-flow canary for issue #187: a later subcommand must resolve the
# container an earlier one created, using ONLY --workspace-folder (no
# --container-id, no --id-label). This exercises the pure workspace + config
# container-identity path that regressed when `up` and `exec`/`run-user-commands`
# computed different `devcontainer.configHash` values for the same config.
#
# README mapping ("Compound flow: up -> exec -> run-user-commands -> down"):
#   1. up                  creates the container; postCreate writes a marker
#   2. exec                resolves it by --workspace-folder, reads the marker
#   3. run-user-commands   resolves the same container (shared identity path)
#   4. down                resolves and removes it by --workspace-folder

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
MARKER="up-exec-down-ok"

# Identity resolution is anchored to --workspace-folder for every subcommand,
# which is exactly what this canary verifies — so this is all exec /
# run-user-commands / down need to find up's container.
#
# Every subcommand must ALSO derive the same in-container workspace folder as
# `up` mounted, because `exec`/`run-user-commands` chdir into it before running.
# Since #273/#309 that derivation is governed by `--mount-workspace-git-root`,
# but `run-user-commands` (and `down`) don't accept that flag while `up`/`exec`
# do — so passing `--mount-workspace-git-root false` only to some subcommands
# makes them disagree on the path and `chdir` fails (rc 127). The one choice that
# is consistent across ALL four here is the default (git-root anchoring): inside
# this monorepo `up` mounts the git root and every subcommand derives the same
# `/workspaces/<repo>/examples/up/up-exec-down` cwd. (The identity marker lives at
# `/tmp`, so the mount location doesn't affect what this canary asserts.)
WS_ARGS=(--workspace-folder "$SCRIPT_DIR")
UP_ARGS=("${WS_ARGS[@]}")

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	"$DEACON_BIN" down "${WS_ARGS[@]}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== up: create the container ==" >&2
run "$DEACON_BIN" up "${UP_ARGS[@]}" --remove-existing-container >/dev/null

echo "== exec: resolve up's container by --workspace-folder (no --container-id) ==" >&2
got="$(run "$DEACON_BIN" exec "${WS_ARGS[@]}" cat /tmp/identity-marker 2>/dev/null | tr -d '[:space:]')"
if [ "$got" != "$MARKER" ]; then
	echo "FAIL: exec --workspace-folder did not resolve up's container (got '$got', want '$MARKER')" >&2
	echo "      This is the #187 configHash-mismatch regression." >&2
	exit 1
fi
echo "exec resolved up's container by --workspace-folder ✓" >&2

echo "== run-user-commands: same identity resolution path ==" >&2
run "$DEACON_BIN" run-user-commands "${WS_ARGS[@]}" >/dev/null
echo "run-user-commands resolved the same container ✓" >&2

echo "== down: resolve and remove by --workspace-folder ==" >&2
run "$DEACON_BIN" down "${WS_ARGS[@]}" >/dev/null
echo "down removed the container ✓" >&2

trap - EXIT
echo "up-exec-down canary passed ✓" >&2
