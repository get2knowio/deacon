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

cd "$SCRIPT_DIR"

extract_variant() {
	"$PYTHON_BIN" -c 'import json, sys; doc = json.loads(sys.stdin.read()); env = doc.get("configuration", {}).get("containerEnv", {}) or {}; print(env.get("VARIANT", ""))'
}

assert_variant() {
	local label="$1" expected="$2"
	shift 2
	echo "== ${label} ==" >&2
	local out
	out="$(run "$DEACON_BIN" read-configuration "$@" 2>/dev/null)"
	local actual
	actual="$(printf '%s' "$out" | extract_variant)"
	if [ "$actual" != "$expected" ]; then
		echo "FAIL: ${label} expected VARIANT=${expected}, got '${actual}'" >&2
		exit 1
	fi
	echo "  ok: VARIANT=${actual}" >&2
}

assert_variant "Default discovery" default \
	--workspace-folder "$SCRIPT_DIR"

assert_variant "Named: python" python \
	--workspace-folder "$SCRIPT_DIR" \
	--config "$SCRIPT_DIR/.devcontainer/python/devcontainer.json"

assert_variant "Named: rust" rust \
	--workspace-folder "$SCRIPT_DIR" \
	--config "$SCRIPT_DIR/.devcontainer/rust/devcontainer.json"

echo "All scenarios passed." >&2
