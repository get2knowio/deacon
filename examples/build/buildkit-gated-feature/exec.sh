#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG="${IMAGE_TAG:-myorg/buildkit-gated:latest}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker rmi -f "$IMAGE_TAG" >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build gated feature (README: Usage) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name "$IMAGE_TAG" --output-format json "$@"
