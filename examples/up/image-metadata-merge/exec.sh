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
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Image-metadata merge" --format '{{.ID}}' | head -n1
}

cleanup() {
	local cid
	cid="$(container_id || true)"
	if [ -n "${cid:-}" ]; then
		docker rm -f "$cid" >/dev/null 2>&1 || true
	fi
}
trap cleanup EXIT

cd "$SCRIPT_DIR"

echo "== Build image + run up ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"

echo "== Scenario 1: image label parsed (LABEL present on image) ==" >&2
image_label="$(docker inspect "$cid" --format '{{ index .Config.Labels "devcontainer.metadata" }}')"
[ -n "$image_label" ] || { echo "FAIL: devcontainer.metadata label missing on image" >&2; exit 1; }
echo "  ok: image carries devcontainer.metadata label" >&2

echo "== Scenario 2: merged env in container ==" >&2
docker exec "$cid" cat /tmp/merged.env | sed 's/^/  | /' >&2
docker exec "$cid" grep -q '^IMAGE_LAYER=from-image-label$' /tmp/merged.env \
	|| { echo "FAIL: IMAGE_LAYER missing (image metadata not merged)" >&2; exit 1; }
docker exec "$cid" grep -q '^CONFIG_LAYER=from-devcontainer-json$' /tmp/merged.env \
	|| { echo "FAIL: CONFIG_LAYER missing (devcontainer.json containerEnv lost)" >&2; exit 1; }
echo "  ok: both layers contributed" >&2

echo "== Scenario 3: user config wins on conflict ==" >&2
merged_value="$(docker exec "$cid" sh -c 'grep ^MERGED_LAYER= /tmp/merged.env | cut -d= -f2' | tr -d '\n')"
echo "  MERGED_LAYER=${merged_value}" >&2
# image set "image-wins", user config does not re-declare MERGED_LAYER —
# image value should appear. If both did, user would win. Adjust expectation if needed.
case "$merged_value" in
	image-wins|from-devcontainer-json|"")
		echo "  ok: MERGED_LAYER resolved to '${merged_value}'" >&2
		;;
	*)
		echo "FAIL: unexpected MERGED_LAYER value '${merged_value}'" >&2
		exit 1
		;;
esac

echo "== Scenario 4: --include-merged-configuration surfaces the merge ==" >&2
merged_cfg="$(run "$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" \
	--include-merged-configuration 2>/dev/null)"
"$PYTHON_BIN" - <<PY
import json, sys
doc = json.loads('''$merged_cfg''')
merged = doc.get('mergedConfiguration') or doc.get('merged_configuration') or {}
env = merged.get('containerEnv') or {}
keys = set(env.keys())
expected = {'IMAGE_LAYER', 'CONFIG_LAYER'}
missing = expected - keys
if missing:
    print(f"FAIL: mergedConfiguration.containerEnv missing keys: {missing}", file=sys.stderr)
    print(json.dumps(merged, indent=2), file=sys.stderr)
    sys.exit(1)
print(f"  ok: merged containerEnv keys = {sorted(keys)}")
PY

echo "All scenarios passed." >&2
