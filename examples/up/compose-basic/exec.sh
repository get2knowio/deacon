#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

# README: "Basic Compose Up" — start app + db services and run postCreateCommand.
run() {
	echo "+ $*" >&2
	"$@"
}

cd "$SCRIPT_DIR"
# Run against docker-compose.yml targeting service=app as described in the README.
echo "== Compose Basic: app + db services (README: Basic Compose Up) ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@"
docker compose down -v >/dev/null 2>&1 || true
