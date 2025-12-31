#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"

run_expect_fail() {
	echo "+ $*" >&2
	set +e
	"$@"
	code=$?
	set -e
	if [ $code -eq 0 ]; then
		echo "Expected failure but command succeeded" >&2
		exit 1
	fi
	echo "Received expected failure (exit $code)" >&2
}

cleanup() {
	rm -rf "$OUTPUT_DIR"
}

trap cleanup EXIT

mkdir -p "$OUTPUT_DIR"

echo "== Invalid feature packaging should fail (README: Usage) ==" >&2
run_expect_fail deacon features package "$SCRIPT_DIR" --output-folder "$OUTPUT_DIR" "$@"
