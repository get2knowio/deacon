#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

EXAMPLES=(
	"basic-dockerfile"
	"buildkit-gated-feature"
	"compose-missing-service"
	"compose-service-target"
	"compose-unsupported-flags"
	"compose-with-features"
	"dockerfile-with-features"
	"duplicate-tags"
	"image-reference"
	"image-reference-with-features"
	"invalid-config-name"
	"multi-tags-and-labels"
	"output-archive"
	"platform-and-cache"
	"push"
	"push-output-conflict"
	"secrets-and-ssh"
	"unwritable-output"
)

for example in "${EXAMPLES[@]}"; do
	script_path="${SCRIPT_DIR}/${example}/exec.sh"
	if [ ! -f "$script_path" ]; then
		echo "Missing exec script for example: ${example}" >&2
		exit 1
	fi

	echo "=== Running build example: ${example} ===" >&2
	(
		cd "${SCRIPT_DIR}/${example}"
		"$script_path"
	)
done
