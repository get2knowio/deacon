#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

EXAMPLES=(
	"manifest-public-registry"
	"manifest-local-feature"
	"manifest-json-output"
	"tags-public-feature"
	"tags-json-output"
	"dependencies-simple"
	"dependencies-complex"
	"verbose-text-output"
	"verbose-json-output"
	"error-handling-invalid-ref"
	"error-handling-network-failure"
	"local-feature-only-manifest"
)

for example in "${EXAMPLES[@]}"; do
	script_path="${SCRIPT_DIR}/${example}/exec.sh"
	if [ ! -f "$script_path" ]; then
		echo "Missing exec script for example: ${example}" >&2
		exit 1
	fi

	echo "=== Running features-info example: ${example} ===" >&2
	(
		cd "${SCRIPT_DIR}/${example}"
		"$script_path"
	)
done
