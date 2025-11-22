#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

EXAMPLES=(
	"single-feature"
	"collection-basic"
	"force-clean"
	"invalid-feature"
	"non-ascii"
)

for example in "${EXAMPLES[@]}"; do
	script_path="${SCRIPT_DIR}/${example}/exec.sh"
	if [ ! -f "$script_path" ]; then
		echo "Missing exec script for example: ${example}" >&2
		exit 1
	fi

	echo "=== Running feature-package example: ${example} ===" >&2
	(
		cd "${SCRIPT_DIR}/${example}"
		"$script_path"
	)
done
