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

# README mapping:
# - "Basic Remote Environment"      -> devcontainer.json remoteEnv (always applied)
# - "Add Runtime Environment Vars"  -> three --remote-env flags below
# - "Load from Secrets File"        -> secrets.env
# - "Combine Config, Flags, Secrets"-> secrets.env + env.env + remote-env flags
run() {
	echo "+ $*" >&2
	"$@"
}

extract_container_id() {
	printf '%s' "$1" | "$PYTHON_BIN" - <<'PY'
import json, sys
data = json.load(sys.stdin)
print(data.get("containerId", ""))
PY
}

cleanup_container() {
	if [ -n "$1" ]; then
		docker rm -f "$1" >/dev/null 2>&1 || true
	fi
}

cd "$SCRIPT_DIR"

# Basic devcontainer remoteEnv only.
echo "== Basic Remote Environment (devcontainer.json only) ==" >&2
out_basic="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@")"
cleanup_container "$(extract_container_id "$out_basic")"

# Add runtime environment variables.
echo "== Add Runtime Environment Variables ==" >&2
out_runtime="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--remote-env "API_ENDPOINT=https://api.example.com" \
	--remote-env "FEATURE_FLAG_BETA=true" \
	--remote-env "MAX_WORKERS=4" \
	"$@")"
cleanup_container "$(extract_container_id "$out_runtime")"

# Load from secrets file only.
echo "== Load from Secrets File (secrets.env) ==" >&2
out_secrets_only="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--secrets-file "${SCRIPT_DIR}/secrets.env" \
	"$@")"
cleanup_container "$(extract_container_id "$out_secrets_only")"

# Combine config, flags, and secrets files.
echo "== Combine Config, Flags, and Secrets ==" >&2
out_combined="$(run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container \
	--remote-env "API_ENDPOINT=https://api.example.com" \
	--remote-env "FEATURE_FLAG_BETA=true" \
	--remote-env "MAX_WORKERS=4" \
	--secrets-file "${SCRIPT_DIR}/secrets.env" \
	--secrets-file "${SCRIPT_DIR}/env.env" \
	"$@")"
cleanup_container "$(extract_container_id "$out_combined")"
