#!/usr/bin/env bash
set -euo pipefail

# Canary: the Output Streams Contract (CLAUDE.md "Output Streams Contract").
#
# Contract under test (hermetic — uses `read-configuration`, no Docker):
#   JSON modes (`--log-format json`):
#     - stdout: a SINGLE JSON document only (the command result)
#     - stderr: all logs/diagnostics, as newline-delimited JSON objects
#   Text mode (default):
#     - stdout: the result; stderr: logs
#   Critically, logs flowing on stderr must NEVER pollute the stdout JSON.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

cd "$SCRIPT_DIR"

# Scenario 1: JSON mode keeps stdout a single valid JSON document.
echo "== Scenario 1: --log-format json -> stdout is one JSON document ==" >&2
run "$DEACON_BIN" --log-format json read-configuration \
	--workspace-folder "$SCRIPT_DIR" \
	>/tmp/json-logs-out.json 2>/tmp/json-logs-err.log
python3 -c 'import json,sys; json.load(open("/tmp/json-logs-out.json"))' || {
	echo "FAIL: stdout is not a single valid JSON document" >&2
	head -c 400 /tmp/json-logs-out.json >&2
	exit 1
}
echo "  ok: stdout parsed as JSON" >&2

# Scenario 2: with debug logging on, stderr carries JSON log objects AND
# stdout stays a clean single JSON document (logs must not leak to stdout).
echo "== Scenario 2: debug logs go to stderr as JSON; stdout stays clean ==" >&2
run "$DEACON_BIN" --log-format json --log-level debug read-configuration \
	--workspace-folder "$SCRIPT_DIR" \
	>/tmp/json-logs-out2.json 2>/tmp/json-logs-err2.log
python3 -c 'import json,sys; json.load(open("/tmp/json-logs-out2.json"))' || {
	echo "FAIL: stdout JSON corrupted when logs are emitted" >&2
	head -c 400 /tmp/json-logs-out2.json >&2
	exit 1
}
# Every non-blank stderr line must be a JSON object with a timestamp.
python3 - "$@" <<'PY' || exit 1
import json, sys
lines = [l for l in open("/tmp/json-logs-err2.log") if l.strip()]
if not lines:
    print("FAIL: expected JSON log lines on stderr, got none", file=sys.stderr)
    sys.exit(1)
for i, l in enumerate(lines):
    try:
        d = json.loads(l)
    except json.JSONDecodeError:
        print(f"FAIL: stderr line {i} is not JSON: {l!r}", file=sys.stderr)
        sys.exit(1)
    if "timestamp" not in d:
        print(f"FAIL: stderr JSON log missing 'timestamp': {sorted(d)}", file=sys.stderr)
        sys.exit(1)
print(f"  ok: {len(lines)} JSON log lines on stderr, stdout clean", file=sys.stderr)
PY

# Scenario 3: text mode (default) — stdout is the human result, and is NOT
# JSON-log noise. We only assert stdout is non-empty and stderr/stdout are
# separate streams (logs never appear on stdout).
echo "== Scenario 3: text mode keeps result on stdout ==" >&2
run "$DEACON_BIN" read-configuration \
	--workspace-folder "$SCRIPT_DIR" \
	>/tmp/json-logs-text.out 2>/tmp/json-logs-text.err
[ -s /tmp/json-logs-text.out ] || {
	echo "FAIL: text-mode stdout was empty" >&2
	exit 1
}
# A JSON log object should never appear on stdout in any mode.
if grep -q '"timestamp"' /tmp/json-logs-text.out; then
	echo "FAIL: JSON log leaked onto stdout in text mode" >&2
	exit 1
fi
echo "  ok: result on stdout, no log leakage" >&2

rm -f /tmp/json-logs-out.json /tmp/json-logs-err.log \
	/tmp/json-logs-out2.json /tmp/json-logs-err2.log \
	/tmp/json-logs-text.out /tmp/json-logs-text.err

echo "All scenarios passed." >&2
