#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	rm -rf "$OUTPUT_DIR"
}

trap cleanup EXIT

mkdir -p "$OUTPUT_DIR"

echo "== Package collection with non-ASCII path (README: Usage) ==" >&2
run deacon features package "$SCRIPT_DIR" --output-folder "$OUTPUT_DIR" "$@"

echo "== Output artifacts ==" >&2
ls -la "$OUTPUT_DIR"
