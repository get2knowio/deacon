#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"
OLD_ARTIFACT="${OUTPUT_DIR}/OLD_ARTIFACT.tgz"
PLACEHOLDER_CONTENT="placeholder"

run() {
	echo "+ $*" >&2
	"$@"
}

restore_placeholder() {
	mkdir -p "$OUTPUT_DIR"
	printf "%s" "$PLACEHOLDER_CONTENT" > "$OLD_ARTIFACT"
}

cleanup() {
	rm -rf "$OUTPUT_DIR"
	# Restore the placeholder artifact so the repo remains unchanged after the script.
	restore_placeholder
}

trap cleanup EXIT

mkdir -p "$OUTPUT_DIR"
restore_placeholder

echo "== Package without force-clean (README: Without force-clean) ==" >&2
run deacon features package "$SCRIPT_DIR" --output-folder "$OUTPUT_DIR" "$@"
echo "--- output after non-force build ---" >&2
ls -la "$OUTPUT_DIR"

echo "== Package with -f (README: With force-clean) ==" >&2
run deacon features package "$SCRIPT_DIR" -f --output-folder "$OUTPUT_DIR" "$@"
echo "--- output after force-clean build ---" >&2
ls -la "$OUTPUT_DIR"
