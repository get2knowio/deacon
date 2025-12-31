#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker images --filter "label=example.type=platform-and-cache" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Default build (README: Default Build) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" "$@"

echo "== Build without cache (README: Build Without Cache) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --no-cache "$@"

echo "== Build for specific platform (README: Build for Specific Platform) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --platform linux/amd64 "$@"
