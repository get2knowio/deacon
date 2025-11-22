#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG1="${IMAGE_TAG1:-myorg/multi:dev}"
IMAGE_TAG2="${IMAGE_TAG2:-myorg/multi:latest}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker rmi -f "$IMAGE_TAG1" "$IMAGE_TAG2" >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Multi tags and labels (README: Usage) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" \
	--image-name "$IMAGE_TAG1" \
	--image-name "$IMAGE_TAG2" \
	--label env=dev \
	--label team=platform \
	--output-format json \
	"$@"
