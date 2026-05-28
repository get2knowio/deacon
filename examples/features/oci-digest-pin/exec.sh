#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Feature ref pinned by @sha256 digest" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
	# Restore placeholder so subsequent runs aren't pinned to a stale digest.
	if [ -f "$SCRIPT_DIR/devcontainer.json.bak" ]; then
		mv "$SCRIPT_DIR/devcontainer.json.bak" "$SCRIPT_DIR/devcontainer.json"
	fi
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Scenario 1: resolve current digest from ghcr.io ==" >&2
if ! DIGEST="$(docker manifest inspect ghcr.io/devcontainers/features/git:1 --verbose 2>/dev/null \
	| python3 -c 'import json,sys; data=json.load(sys.stdin); d=data[0] if isinstance(data, list) else data; print(d["Descriptor"]["digest"])' 2>/dev/null)"; then
	echo "SKIP: could not resolve digest (no registry access?)" >&2
	exit 0
fi
if [ -z "${DIGEST:-}" ] || ! [[ "$DIGEST" == sha256:* ]]; then
	echo "SKIP: unexpected manifest format (got '${DIGEST}')" >&2
	exit 0
fi
echo "  resolved digest: ${DIGEST}" >&2

cp devcontainer.json devcontainer.json.bak
sed -i.tmp "s|@sha256:REPLACE_WITH_REAL_DIGEST|@${DIGEST}|" devcontainer.json
rm -f devcontainer.json.tmp

echo "== Scenario 2: read-configuration shows the pinned ref ==" >&2
# The config lives at the workspace root as devcontainer.json (not a spec
# discovery location), so point --config at it explicitly — same as Scenario 3.
cfg="$(run "$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--config "$SCRIPT_DIR/devcontainer.json" \
	--include-features-configuration 2>/dev/null)"
if ! printf '%s' "$cfg" | grep -q "$DIGEST"; then
	echo "FAIL: digest not present in resolved configuration" >&2
	exit 1
fi
echo "  ok: digest present in configuration" >&2

echo "== Scenario 3: up installs the pinned feature ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --config "$SCRIPT_DIR/devcontainer.json" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
docker exec "$cid" git --version | sed 's/^/  git: /' >&2

echo "All scenarios run." >&2
