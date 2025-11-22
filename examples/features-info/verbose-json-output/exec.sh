#!/usr/bin/env bash
set -euo pipefail

if [ "${DEACON_NETWORK_TESTS:-}" != "1" ]; then
	echo "Skipping: DEACON_NETWORK_TESTS=1 required for registry access" >&2
	exit 0
fi

echo "== Verbose JSON output (README: Running) ==" >&2
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json "$@"

echo "== Extract canonicalId with jq (README: Extract specific fields) ==" >&2
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json "$@" | jq '.canonicalId'

echo "== Count published tags with jq (README: Extract specific fields) ==" >&2
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json "$@" | jq '.publishedTags | length'
