#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# README (examples/up): "Quick Start" and per-category tables. This runner sequentially invokes each example's exec.sh.
DEFAULT_EXAMPLES=(
	"basic-image"
	"dockerfile-build"
	"with-features"
	"compose-basic"
	"compose-profiles"
	"lifecycle-hooks"
	"prebuild-mode"
	"skip-lifecycle"
	"dotfiles-integration"
	"additional-mounts"
	"remote-env-secrets"
	"configuration-output"
	"id-labels-reconnect"
	"remove-existing"
	"gpu-modes"
)

if [ -n "${EXAMPLES_OVERRIDE:-}" ]; then
	read -r -a EXAMPLES_TO_RUN <<<"$EXAMPLES_OVERRIDE"
else
	EXAMPLES_TO_RUN=("${DEFAULT_EXAMPLES[@]}")
fi

for example in "${EXAMPLES_TO_RUN[@]}"; do
	script_path="${SCRIPT_DIR}/${example}/exec.sh"
	if [ ! -f "$script_path" ]; then
		echo "Missing exec script for example: ${example}" >&2
		exit 1
	fi

	# Each child script contains comments tying the commands to its README sections.
	echo "=== Running example: ${example} ===" >&2
	(
		cd "${SCRIPT_DIR}/${example}"
		"$script_path"
	)
done
