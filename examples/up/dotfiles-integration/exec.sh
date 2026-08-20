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
# - "Basic Dotfiles Integration"  -> default DOTFILES_REPOSITORY
# - "Custom Install Command"      -> DOTFILES_INSTALL_COMMAND
# - "Custom Target Path"          -> DOTFILES_TARGET_PATH
# - "All Options Combined"        -> set all three variables.
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" -c 'import json, sys; data = json.load(sys.stdin); print(data.get("containerId", ""))'
}

cleanup_container() {
	if [ -n "$1" ]; then
		docker rm -f "$1" >/dev/null 2>&1 || true
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
trap clean_workspace_artifacts EXIT

cd "$SCRIPT_DIR"

echo "== Basic Dotfiles Integration (default repository) ==" >&2
out_basic="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--dotfiles-repository "https://github.com/holman/dotfiles" \
	"$@")"
cleanup_container "$(extract_container_id "$out_basic")"

echo "== Custom Install Command ==" >&2
out_custom_install="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--dotfiles-repository "https://github.com/holman/dotfiles" \
	--dotfiles-install-command "echo 'custom install command executed'" \
	"$@")"
cleanup_container "$(extract_container_id "$out_custom_install")"

echo "== Custom Target Path ==" >&2
out_custom_target="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--dotfiles-repository "https://github.com/holman/dotfiles" \
	--dotfiles-target-path "~/.config/dotfiles-custom" \
	"$@")"
cleanup_container "$(extract_container_id "$out_custom_target")"

echo "== All Options Combined ==" >&2
out_all="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--dotfiles-repository "https://github.com/holman/dotfiles" \
	--dotfiles-install-command "echo 'custom install command executed'" \
	--dotfiles-target-path "~/.config/dotfiles-custom" \
	"$@")"
cleanup_container "$(extract_container_id "$out_all")"
