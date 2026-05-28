#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

cd "$SCRIPT_DIR"

assert_cycle_rejected() {
	local label="$1" cfg="$2"
	echo "== ${label} (${cfg}) ==" >&2
	set +e
	"$DEACON_BIN" read-configuration \
		--config "$SCRIPT_DIR/$cfg" >/dev/null 2> "$SCRIPT_DIR/.last.stderr"
	local status=$?
	set -e
	sed 's/^/  | /' "$SCRIPT_DIR/.last.stderr" >&2
	if [ $status -eq 0 ]; then
		echo "FAIL: ${cfg} should have been rejected" >&2
		exit 1
	fi
	echo "  ok: non-zero exit (${status})" >&2
	if ! grep -qiE 'cycle|extends|recursive|loop' "$SCRIPT_DIR/.last.stderr"; then
		echo "  note: stderr does not mention cycle/extends explicitly — diagnostic could be improved" >&2
	else
		echo "  ok: diagnostic mentions cycle/extends" >&2
	fi
	if ! grep -qE "(alpha|bravo|self)\.json" "$SCRIPT_DIR/.last.stderr"; then
		echo "  note: stderr does not name a participating file" >&2
	else
		echo "  ok: stderr names a participating file" >&2
	fi
	rm -f "$SCRIPT_DIR/.last.stderr"
}

assert_cycle_rejected "Two-file cycle" alpha.json
assert_cycle_rejected "One-file cycle" self.json

echo "All scenarios passed." >&2
