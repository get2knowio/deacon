#!/usr/bin/env bash
set -euo pipefail

if [ "${DEACON_NETWORK_TESTS:-}" != "1" ]; then
	echo "Skipping: DEACON_NETWORK_TESTS=1 required for registry access" >&2
	exit 0
fi

echo "== Manifest JSON output (README: Running) ==" >&2
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json "$@"

echo "== Parse canonicalId with jq (README: Parse with jq) ==" >&2
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json "$@" | jq '.canonicalId'

echo "== Extract digest only (README: Extract digest only) ==" >&2
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json "$@" | jq -r '.canonicalId' | cut -d'@' -f2
