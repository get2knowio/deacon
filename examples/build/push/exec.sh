#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG1="${IMAGE_TAG1:-ghcr.io/your-org/push-example:dev}"
IMAGE_TAG2="${IMAGE_TAG2:-ghcr.io/your-org/push-example:latest}"

run_allow_fail() {
	echo "+ $*" >&2
	set +e
	"$@"
	code=$?
	set -e
	if [ $code -ne 0 ]; then
		echo "Push command exited $code (may require registry auth)" >&2
	fi
}

cleanup() {
	docker rmi -f "$IMAGE_TAG1" "$IMAGE_TAG2" >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Push to registry (README: Usage) ==" >&2
run_allow_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" \
	--image-name "$IMAGE_TAG1" \
	--image-name "$IMAGE_TAG2" \
	--push \
	--output-format json \
	"$@"
