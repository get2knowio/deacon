#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

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

cd "$SCRIPT_DIR"

echo "== Unsupported --push in compose mode (README) ==" >&2
run_expect_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --push --image-name myorg/compose-error:latest "$@"

echo "== Unsupported --output in compose mode (README) ==" >&2
run_expect_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --output type=oci,dest=out.tar --image-name myorg/compose-error:latest "$@"
