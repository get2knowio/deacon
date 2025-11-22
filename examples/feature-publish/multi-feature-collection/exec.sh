#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

OUT="$(mktemp -d)"
cleanup() {
	rm -rf "$OUT"
}
trap cleanup EXIT

echo "== Package collection (README: Commands) ==" >&2
deacon features package "$SCRIPT_DIR" --output "$OUT" --progress json "$@"
ls -la "$OUT"

echo "== Dry-run publish collection (README: Commands) ==" >&2
deacon features publish "$SCRIPT_DIR" \
	--namespace exampleorg/multi-collection \
	--registry ghcr.io \
	--dry-run \
	--progress json \
	"$@"
