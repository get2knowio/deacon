#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	# Deacon-created compose containers don't carry deacon's name labels —
	# only the standard docker-compose ones. Filter by compose project +
	# service. Deacon derives the project name from the workspace dir's
	# basename.
	local project
	project="$(basename "$SCRIPT_DIR")"
	docker ps -a --filter "label=com.docker.compose.project=${project}" \
		--filter "label=com.docker.compose.service=app" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		"$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --remove >/dev/null 2>&1 || true
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Bring compose project up (merging both files) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Read merged env ==" >&2
docker exec "$cid" cat /tmp/merged.env | sed 's/^/  | /' >&2

echo "== Scenario 1: both files contributed ==" >&2
for kv in "BASE=from-base" "OVERRIDE=from-override"; do
	docker exec "$cid" grep -q "^${kv}$" /tmp/merged.env \
		|| { echo "FAIL: ${kv} missing from merged env" >&2; exit 1; }
	echo "  ok: ${kv}" >&2
done

echo "== Scenario 2: later file wins on FINAL ==" >&2
docker exec "$cid" grep -q '^FINAL=override-wins$' /tmp/merged.env \
	|| { echo "FAIL: FINAL=override-wins missing (base-wins should be overridden)" >&2; exit 1; }
echo "  ok: FINAL=override-wins" >&2

echo "All scenarios passed." >&2
