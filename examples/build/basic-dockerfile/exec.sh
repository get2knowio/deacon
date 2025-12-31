#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup_images() {
	docker images --filter "label=example.type=basic-dockerfile" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
}

trap cleanup_images EXIT

cd "$SCRIPT_DIR"

echo "== Basic build (README: Basic Build) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" "$@"

echo "== Build with custom build arg (README: Build with Custom Build Args) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --build-arg FOO=BAR "$@"

echo "== Build with JSON output (README: Build with JSON Output) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --build-arg FOO=BAR --output-format json "$@"
