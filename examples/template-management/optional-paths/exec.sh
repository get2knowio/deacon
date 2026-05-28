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

DEST="$(mktemp -d)"
cleanup() { rm -rf "$DEST"; }
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Scenario 1: optionalPaths is parseable + non-empty ==" >&2
"$PYTHON_BIN" - "$SCRIPT_DIR/devcontainer-template.json" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    meta = json.load(f)
opt = meta.get("optionalPaths", [])
files = meta.get("files", [])
assert isinstance(opt, list) and opt, "optionalPaths must be a non-empty array"
for p in opt:
    assert p in files, f"optionalPath {p} not listed in files[]"
print(f"  ok: {len(opt)} optionalPaths, all referenced by files[]")
PY

echo "== Scenario 2: apply template + substitute projectName ==" >&2
# `templates apply` doesn't accept up/down-style flags; don't forward `$@`.
run "$DEACON_BIN" templates apply "$SCRIPT_DIR" \
	--output "$DEST" \
	--option projectName=acme-svc >/dev/null

echo "== Scenario 3: required files always present ==" >&2
for path in .devcontainer/devcontainer.json PROJECT_README.md scripts/setup.sh; do
	[ -f "$DEST/$path" ] || { echo "FAIL: required file ${path} missing" >&2; exit 1; }
	echo "  ok: ${path}" >&2
done

echo "== Scenario 4: option substitution occurred ==" >&2
if grep -q '${templateOption:projectName}' "$DEST/.devcontainer/devcontainer.json"; then
	echo "FAIL: placeholder not substituted" >&2
	exit 1
fi
grep -q 'acme-svc' "$DEST/.devcontainer/devcontainer.json" \
	|| { echo "FAIL: projectName substitution missing" >&2; exit 1; }
echo "  ok: projectName substituted" >&2

echo "== Scenario 5: optional files present under default apply ==" >&2
for path in scripts/db-migrate.sh docs/CONTRIBUTING.md .github/workflows/ci.yml; do
	if [ -f "$DEST/$path" ]; then
		echo "  ok: ${path} included (default)" >&2
	else
		echo "  note: ${path} absent — IDE-driven opt-out flow" >&2
	fi
done

echo "All scenarios run." >&2
