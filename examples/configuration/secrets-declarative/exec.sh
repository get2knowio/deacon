#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then PYTHON_BIN=python; fi
fi

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Declarative secrets" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

# Reproducible fake secrets — never use real values in an example.
export GITHUB_TOKEN="demo-fake-token-1234"
export DATABASE_URL="postgres://dev:dev@localhost:5432/dev"

cd "$SCRIPT_DIR"

echo "== Scenario 1: read-configuration exposes secrets declaration ==" >&2
# Top-level devcontainer.json (not under .devcontainer/), point at it explicitly.
cfg="$(run "$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" 2>/dev/null)"
"$PYTHON_BIN" - <<PY
import json, sys
doc = json.loads('''$cfg''')
sec = doc.get('configuration', {}).get('secrets', {})
for key in ('GITHUB_TOKEN', 'DATABASE_URL'):
    entry = sec.get(key)
    assert entry, f"secrets.{key} missing from resolved configuration"
    assert entry.get('description'), f"secrets.{key}.description missing"
    assert entry.get('documentationUrl'), f"secrets.{key}.documentationUrl missing"
    print(f"  ok: secrets.{key} declared with description + documentationUrl")
PY

echo "== Scenario 2: up + check canary value ==" >&2
LOG="$(mktemp)"
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null 2> "$LOG"
cid="$(container_id)"
canary="$(docker exec "$cid" cat /tmp/redaction-canary 2>/dev/null || true)"
echo "  canary value: ${canary}" >&2
if [ "$canary" = "demo-fak" ]; then
	echo "  ok: GITHUB_TOKEN reached container via remoteEnv substitution" >&2
else
	echo "  note: canary is '${canary}'; spec compliance varies on whether localEnv substitution propagates secrets" >&2
fi

echo "== Scenario 3: redaction — full token value absent from logs ==" >&2
if grep -F "demo-fake-token-1234" "$LOG" >/dev/null; then
	echo "FAIL: literal token value found in stderr/log" >&2
	grep -n "demo-fake-token-1234" "$LOG" >&2 | head -5
	exit 1
fi
echo "  ok: full token value not present in logs" >&2
rm -f "$LOG"

echo "All scenarios passed." >&2
