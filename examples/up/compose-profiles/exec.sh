#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

# README: demonstrates default (no profiles), dev, test, prod, and combined profiles.
run() {
	echo "+ $*" >&2
	"$@"
}

cd "$SCRIPT_DIR"
EXTRA_ARGS=("$@")

run_profile() {
	local profile_env="$1"
	local label="$2"
	echo "== ${label} ==" >&2
	if [ -n "$profile_env" ]; then
		run COMPOSE_PROFILES="$profile_env" "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "${EXTRA_ARGS[@]}"
	else
		run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "${EXTRA_ARGS[@]}"
	fi
	docker compose down -v >/dev/null 2>&1 || true
}

# README default path: no profiles
run_profile "" "Default (No Profiles) -> services: app, cache"
# README: Development Profile
run_profile "dev" "Development Profile -> services: app, cache, mailcatcher"
# README: Multiple Profiles (dev,test)
run_profile "dev,test" "Multiple Profiles (dev,test) -> services: app, cache, mailcatcher, test-db"
# README: Testing Profile Only
run_profile "test" "Testing Profile Only -> services: app, cache, test-db"
# README: Production Simulation (prod)
run_profile "prod" "Production Simulation (prod) -> services: app, cache, nginx reverse proxy"
