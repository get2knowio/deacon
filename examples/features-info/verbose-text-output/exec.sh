#!/usr/bin/env bash
set -euo pipefail

if [ "${DEACON_NETWORK_TESTS:-}" != "1" ]; then
	echo "Skipping: DEACON_NETWORK_TESTS=1 required for registry access" >&2
	exit 0
fi

echo "== Verbose text output (README: Running) ==" >&2
deacon features info verbose ghcr.io/devcontainers/features/node:1 "$@"

echo "== Verbose with debug logging (README: With debug logging) ==" >&2
deacon features info verbose ghcr.io/devcontainers/features/node:1 --log-level debug "$@"
