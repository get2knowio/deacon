#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

run_expect_fail() {
	echo "+ $*" >&2
	set +e
	"$@"
	code=$?
	set -e
	if [ $code -eq 0 ]; then
		echo "Expected cycle error but command succeeded" >&2
		exit 1
	fi
	echo "Received expected failure (exit $code)" >&2
}

echo "== Cycle detection (README: Expected result is error) ==" >&2
run_expect_fail deacon features plan --config "$SCRIPT_DIR/devcontainer.json" --json "$@"
