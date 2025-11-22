#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

cleanup() {
	docker images --filter "label=example.type=compose-service-target" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
	docker rmi -f myapp:latest >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build targeted service (README: Build the targeted service) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" "$@"

echo "== Build with custom tag (README: Build with custom tags) ==" >&2
run "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --image-name myapp:latest "$@"
