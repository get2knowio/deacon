#!/usr/bin/env bash
set -euo pipefail

if [ "${DEACON_NETWORK_TESTS:-}" != "1" ]; then
	echo "Skipping: DEACON_NETWORK_TESTS=1 required for registry access" >&2
	exit 0
fi

echo "== Tags JSON output (README: Running) ==" >&2
deacon features info tags ghcr.io/devcontainers/features/node --output-format json "$@"

echo "== Filter latest 1.x patch with jq (README: Filter with jq) ==" >&2
deacon features info tags ghcr.io/devcontainers/features/node --output-format json "$@" \
	| jq -r '.publishedTags[]' | grep '^1\.' | sort -V | tail -1

echo "== Check if specific version exists (README: Check specific version) ==" >&2
deacon features info tags ghcr.io/devcontainers/features/node --output-format json "$@" \
	| jq '.publishedTags[] | select(. == "1.2.0")'
