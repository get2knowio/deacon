#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG="${IMAGE_TAG:-myimage:latest}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker rmi -f "$IMAGE_TAG" >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build from image reference (README: Build) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name "$IMAGE_TAG" "$@"

echo "== Build with custom tags and labels (README: Build with custom tags and labels) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" \
	--image-name "$IMAGE_TAG" \
	--label "version=1.0" \
	"$@"
