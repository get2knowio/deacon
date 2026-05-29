#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
LABEL="canary.group=msdown"

run() {
	echo "+ $*" >&2
	"$@"
}

# All services in docker-compose.yml carry label canary.group=msdown, so we can
# find them regardless of deacon's derived compose project name.
service_ids() {
	docker ps -a --filter "label=${LABEL}" --format '{{.ID}}'
}
running_count() {
	docker ps --filter "label=${LABEL}" --filter "status=running" -q | wc -l | tr -d ' '
}
present_count() {
	docker ps -a --filter "label=${LABEL}" -q | wc -l | tr -d ' '
}

cleanup() {
	service_ids | xargs -r docker rm -f >/dev/null 2>&1 || true
	docker volume ls -q | grep -i msdown-data | xargs -r docker volume rm >/dev/null 2>&1 || true
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

up() {
	run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" "$@" >/dev/null
}

# Scenario 1: up starts both services.
echo "== Scenario 1: up brings up app + db ==" >&2
up
[ "$(running_count)" -eq 2 ] || { echo "FAIL: expected 2 running services, got $(running_count)" >&2; exit 1; }
echo "  ok: 2 services running" >&2

# Scenario 2: shutdownAction=stopCompose -> plain `down` stops but keeps them.
echo "== Scenario 2: down (stopCompose) stops but does not remove ==" >&2
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR"
[ "$(running_count)" -eq 0 ] || { echo "FAIL: services still running after down" >&2; exit 1; }
[ "$(present_count)" -eq 2 ] || { echo "FAIL: expected 2 stopped services to remain, got $(present_count)" >&2; exit 1; }
echo "  ok: services stopped, still present" >&2

# Scenario 3: down --remove deletes the project's containers.
echo "== Scenario 3: down --remove removes containers ==" >&2
up
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --remove
[ "$(present_count)" -eq 0 ] || { echo "FAIL: containers remain after down --remove, got $(present_count)" >&2; exit 1; }
echo "  ok: containers removed" >&2

# Scenario 4: down --remove --volumes also drops the named volume.
echo "== Scenario 4: down --remove --volumes drops the volume ==" >&2
up
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" --remove --volumes
[ "$(present_count)" -eq 0 ] || { echo "FAIL: containers remain after down --remove --volumes" >&2; exit 1; }
vols="$(docker volume ls -q | grep -ic msdown-data || true)"
[ "$vols" -eq 0 ] || { echo "FAIL: msdown-data volume(s) still present (${vols})" >&2; exit 1; }
echo "  ok: containers and volume removed" >&2

echo "All scenarios passed." >&2
