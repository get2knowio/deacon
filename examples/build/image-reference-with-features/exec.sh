#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG="${IMAGE_TAG:-myorg/feat-image-ref:latest}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker rmi -f "$IMAGE_TAG" >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build image reference with feature (README: Usage) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name "$IMAGE_TAG" --output-format json "$@"

echo "== Verify feature artifact (README: Verify) ==" >&2
run docker run --rm "$IMAGE_TAG" cat /hello.txt
