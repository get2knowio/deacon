#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
IMAGE_TAG="myorg/dups:latest"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker rmi -f "$IMAGE_TAG" >/dev/null 2>&1 || true
	docker images --filter "reference=deacon-build:*" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Duplicate tags are normalized (README: Usage) ==" >&2
OUTPUT="$(run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" \
	--image-name "$IMAGE_TAG" \
	--image-name "$IMAGE_TAG" \
	--output-format json "$@" 2>/dev/null)"

echo "build output: ${OUTPUT}" >&2

# The duplicate tag must collapse to a single entry. `imageName` is always a JSON
# array (matching the reference CLI; see #330), so two identical `--image-name`
# flags yield a one-element array, not a two-element one. Assert the exact shape.
python3 -c '
import json, sys
data = json.loads(sys.argv[1])
assert data["outcome"] == "success", data
name = data["imageName"]
assert name == ["myorg/dups:latest"], "expected de-duplicated single-element array, got: %r" % (name,)
print("OK: imageName de-duplicated to a single-element array")
' "$OUTPUT"
