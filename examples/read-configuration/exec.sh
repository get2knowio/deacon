#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

EXAMPLES=(
	"basic"
	"with-variables"
	"extends-chain"
	"override-config"
	"features-minimal"
	"features-additional"
	"compose"
	"legacy-normalization"
	"id-labels-and-devcontainerId"
)

for example in "${EXAMPLES[@]}"; do
	script_path="${SCRIPT_DIR}/${example}/exec.sh"
	if [ ! -f "$script_path" ]; then
		echo "Missing exec script for example: ${example}" >&2
		exit 1
	fi

	echo "=== Running read-configuration example: ${example} ===" >&2
	(
		cd "${SCRIPT_DIR}/${example}"
		"$script_path"
	)
done
