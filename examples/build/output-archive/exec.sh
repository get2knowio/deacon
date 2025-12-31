#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG="${IMAGE_TAG:-myorg/archive:exp}"
ARCHIVE_PATH="${ARCHIVE_PATH:-${SCRIPT_DIR}/archive-image.tar}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	rm -f "$ARCHIVE_PATH"
	docker rmi -f "$IMAGE_TAG" >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build with OCI archive output (README: Usage) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name "$IMAGE_TAG" \
	--output "type=oci,dest=${ARCHIVE_PATH}" \
	--output-format json \
	"$@"

echo "== Verify archive contents (README: Verify) ==" >&2
tar tf "$ARCHIVE_PATH" | head
