#!/usr/bin/env bash
set -euo pipefail

if [ "${DEACON_NETWORK_TESTS:-}" != "1" ]; then
	echo "Skipping: DEACON_NETWORK_TESTS=1 required for registry access" >&2
	exit 0
fi

echo "== Tags from public feature (README: Running) ==" >&2
deacon features info tags ghcr.io/devcontainers/features/node "$@"

echo "== Tags with debug logging (README: With debug logging) ==" >&2
deacon features info tags ghcr.io/devcontainers/features/node --log-level debug "$@"
