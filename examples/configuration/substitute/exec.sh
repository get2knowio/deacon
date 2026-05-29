#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

run() {
	echo "+ $*" >&2
	"$@"
}

cd "$SCRIPT_DIR"

# `config substitute` is a no-Docker command: it loads the config, applies
# ${...} variable substitution, and prints the resolved config to stdout.
echo "== Scenario 1: variables are substituted in the output ==" >&2
out="$(CANARY_TOKEN=zzz run "$DEACON_BIN" config substitute --workspace-folder "$SCRIPT_DIR")"
echo "$out" | sed 's/^/  | /' >&2

# localEnv:CANARY_TOKEN -> zzz
echo "$out" | grep -q 'subst-zzz' \
	|| { echo "FAIL: \${localEnv:CANARY_TOKEN} not substituted" >&2; exit 1; }
echo "$out" | grep -q 'hi-zzz' \
	|| { echo "FAIL: \${localEnv} inside containerEnv not substituted" >&2; exit 1; }
# localWorkspaceFolderBasename -> substitute (this dir's name)
echo "$out" | grep -q '/wf/substitute' \
	|| { echo "FAIL: \${localWorkspaceFolderBasename} not substituted" >&2; exit 1; }
echo "  ok: localEnv and localWorkspaceFolderBasename resolved" >&2

echo "== Scenario 2: --dry-run previews without error ==" >&2
CANARY_TOKEN=zzz run "$DEACON_BIN" config substitute --workspace-folder "$SCRIPT_DIR" --dry-run >/dev/null
echo "  ok: --dry-run succeeded" >&2

# Optional stricter assertion if python is available: output is valid JSON.
if command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	echo "== Scenario 3: output is valid JSON with substituted name ==" >&2
	# `config substitute` wraps the resolved config under `.configuration`.
	name="$(printf '%s' "$out" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["configuration"]["name"])' 2>/dev/null || true)"
	[ "$name" = "subst-zzz" ] \
		|| { echo "FAIL: parsed .configuration.name was '${name}', expected 'subst-zzz'" >&2; exit 1; }
	echo "  ok: JSON parses, .configuration.name=subst-zzz" >&2
fi

echo "All scenarios passed." >&2
