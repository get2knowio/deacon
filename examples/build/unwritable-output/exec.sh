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

cleanup() {
	chmod 700 "$SCRIPT_DIR/readonly" >/dev/null 2>&1 || true
	rm -rf "$SCRIPT_DIR/readonly"
	docker rmi -f myorg/unwritable:latest >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

mkdir -p readonly
chmod 500 readonly

echo "== Unwritable output destination (README: expect error) ==" >&2
run_expect_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name myorg/unwritable:latest \
	--output type=oci,dest=readonly/image.tar "$@"
