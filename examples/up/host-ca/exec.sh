#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 (or python) is required to parse JSON output" >&2
		exit 1
	fi
fi
if ! command -v openssl >/dev/null 2>&1; then
	echo "openssl is required to generate the demo corporate CA" >&2
	exit 1
fi

run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" -c 'import json, sys; d = json.load(sys.stdin); print(d.get("containerId", ""))'
}

cd "$SCRIPT_DIR"

# Running inside the deacon monorepo, `up` would mount the git root to the
# workspaceFolder (the --mount-workspace-git-root default), so we pass
# `--mount-workspace-git-root false` to mount this example folder directly.
# Not a deacon bug — see CLAUDE.md "Canary Patterns".
MOUNT_FLAG=(--mount-workspace-git-root false)

# 1. Generate a throwaway "corporate" root CA to inject (an explicit bundle, so
#    the demo doesn't depend on the host machine's trust store).
CA_DIR="$(mktemp -d)"
trap 'rm -rf "$CA_DIR"' EXIT
run openssl req -x509 -newkey rsa:2048 -nodes \
	-keyout "$CA_DIR/corp.key" -out "$CA_DIR/corp-root.pem" -days 3650 \
	-subj "/CN=Example Corp Root CA/O=Example Corp/C=US" \
	-addext "basicConstraints=critical,CA:TRUE" >/dev/null 2>&1

echo "== Host CA: up with --inject-host-ca (README: Injecting a corporate CA) ==" >&2
# The postCreateCommand asserts the injected bundle is present BEFORE it runs —
# i.e. injection happened before any lifecycle hook.
output="$(run "$DEACON_BIN" up \
	--workspace-folder "$SCRIPT_DIR" "${MOUNT_FLAG[@]}" \
	--inject-host-ca "$CA_DIR/corp-root.pem" \
	--remove-existing-container "$@")"
container_id="$(extract_container_id "$output")"
echo "Container: ${container_id}" >&2

# The JSON result includes the injected subjects (additive field).
printf '%s' "$output" | "$PYTHON_BIN" -c \
	'import json,sys; d=json.load(sys.stdin); print("injectedCaSubjects:", d.get("injectedCaSubjects"))' >&2

echo "== Host CA: verify the cert + CA env vars inside the container ==" >&2
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" "${MOUNT_FLAG[@]}" -- \
	sh -c 'head -1 /usr/local/share/deacon/host-ca.crt'
run "$DEACON_BIN" exec --workspace-folder "$SCRIPT_DIR" "${MOUNT_FLAG[@]}" -- \
	sh -c 'echo "SSL_CERT_FILE=$SSL_CERT_FILE"'

# Cleanup: remove the container (the temp CA dir is removed by the EXIT trap).
run "$DEACON_BIN" down --workspace-folder "$SCRIPT_DIR" "${MOUNT_FLAG[@]}" >/dev/null 2>&1 || true
if [ -n "$container_id" ]; then
	docker rm -f "$container_id" >/dev/null 2>&1 || true
fi
echo "== Host CA example complete ==" >&2
