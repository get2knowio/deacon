#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
SECRET_FILE="${SECRET_FILE:-/tmp/test-secret.txt}"

run_allow_fail() {
	echo "+ $*" >&2
	set +e
	"$@"
	code=$?
	set -e
	if [ $code -ne 0 ]; then
		echo "Command exited $code (BuildKit/SSH may be required)" >&2
	fi
}

cleanup() {
	rm -f "$SECRET_FILE"
	# Image tags are not specified; remove by label if present
	docker images --filter "label=example.type=secrets-and-ssh" -q | xargs -r docker rmi -f >/dev/null 2>&1 || true
}

trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build with secret mount (README: Build with Secret Mount) ==" >&2
echo "my-secret-value" > "$SECRET_FILE"
run_allow_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --secret "id=foo,src=${SECRET_FILE}" "$@"

echo "== Build with SSH forwarding (README: Build with SSH Forwarding) ==" >&2
run_allow_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" --ssh default "$@"

echo "== Combine secrets and SSH (README: Combine Secrets and SSH) ==" >&2
run_allow_fail "$DEACON_BIN" build --workspace-folder "$SCRIPT_DIR" \
	--secret "id=foo,src=${SECRET_FILE}" \
	--ssh default \
	"$@"
