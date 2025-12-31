#!/usr/bin/env bash
set -euo pipefail

if [ "${DEACON_NETWORK_TESTS:-}" != "1" ]; then
	echo "Skipping: DEACON_NETWORK_TESTS=1 required for registry access" >&2
	exit 0
fi

echo "== Manifest from public registry (README: Running) ==" >&2
deacon features info manifest ghcr.io/devcontainers/features/node:1 "$@"

echo "== Manifest with debug logging (README: With debug logging) ==" >&2
deacon features info manifest ghcr.io/devcontainers/features/node:1 --log-level debug "$@"
