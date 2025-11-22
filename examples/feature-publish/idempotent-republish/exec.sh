#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== First dry-run publish (README: first run) ==" >&2
deacon features publish "$SCRIPT_DIR" \
	--namespace exampleorg/idempotent \
	--registry ghcr.io \
	--dry-run \
	--progress json \
	"$@"

echo "== Second dry-run publish (README: second run shows skippedTags) ==" >&2
deacon features publish "$SCRIPT_DIR" \
	--namespace exampleorg/idempotent \
	--registry ghcr.io \
	--dry-run \
	--progress json \
	"$@"
