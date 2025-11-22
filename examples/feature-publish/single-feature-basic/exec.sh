#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Package single feature (README: Commands) ==" >&2
OUT="$(mktemp -d)"
deacon features package "$SCRIPT_DIR" --output "$OUT" --progress json "$@"
ls -la "$OUT"

echo "== Dry-run publish with semantic tags (README: Dry-run publish) ==" >&2
deacon features publish "$SCRIPT_DIR" \
	--namespace exampleorg/example-features \
	--registry ghcr.io \
	--dry-run \
	--progress json \
	"$@"

rm -rf "$OUT"
