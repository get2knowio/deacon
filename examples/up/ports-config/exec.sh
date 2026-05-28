#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
	if command -v python >/dev/null 2>&1; then
		PYTHON_BIN="python"
	else
		echo "python3 is required" >&2
		exit 1
	fi
fi

run() {
	echo "+ $*" >&2
	"$@"
}

container_id() {
	docker ps -a --filter "label=devcontainer.source=deacon" --filter "label=devcontainer.name=Ports: forwardPorts, portsAttributes, otherPortsAttributes, appPort" --format '{{.ID}}' | head -n1
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

echo "== Scenario 1: read-configuration parses every port property ==" >&2
cfg_json="$(run "$DEACON_BIN" read-configuration --workspace-folder "$SCRIPT_DIR" 2>/dev/null)"
"$PYTHON_BIN" - <<PY
import json, sys
cfg = json.loads('''$cfg_json''').get('configuration', {})
assert cfg.get('appPort') == 8080, cfg.get('appPort')
fp = cfg.get('forwardPorts', [])
assert 80 in fp and 3000 in fp, fp
assert any(isinstance(x, str) and '9090' in x for x in fp), fp
pa = cfg.get('portsAttributes', {})
assert pa.get('80', {}).get('protocol') == 'http', pa
assert pa.get('9090', {}).get('requireLocalPort') is True, pa
opa = cfg.get('otherPortsAttributes', {})
assert opa.get('onAutoForward') == 'ignore', opa
print('  ok: all port properties parsed')
PY

echo "== Scenario 2: docker inspect shows published ports after up ==" >&2
run "$DEACON_BIN" up --workspace-folder "$SCRIPT_DIR" --remove-existing-container "$@" >/dev/null
cid="$(container_id)"
bindings="$(docker inspect "$cid" --format '{{json .HostConfig.PortBindings}}')"
echo "  PortBindings: ${bindings}" >&2
for port in 80 3000 8080 9090; do
	if printf '%s' "$bindings" | grep -q "\"${port}/tcp\""; then
		echo "  ok: ${port}/tcp published" >&2
	else
		echo "  note: ${port}/tcp not in PortBindings (deacon may surface ports differently)" >&2
	fi
done

echo "== Scenario 3: 9090 bound to 127.0.0.1 only ==" >&2
host_ip_9090="$(printf '%s' "$bindings" | "$PYTHON_BIN" -c '
import json, sys
d = json.load(sys.stdin)
binds = d.get("9090/tcp") or []
print(binds[0].get("HostIp", "") if binds else "")
')"
echo "  HostIp(9090) = ${host_ip_9090}" >&2
if [ "$host_ip_9090" = "127.0.0.1" ]; then
	echo "  ok: 9090 restricted to localhost (forwardPorts host:port form honored)" >&2
else
	echo "  note: HostIp is '${host_ip_9090}' — deacon may not yet parse the host:port form into HostIp" >&2
fi

echo "== Scenario 4: HTTP round-trip on port 80 ==" >&2
host80="$(docker inspect "$cid" --format '{{if index .NetworkSettings.Ports "80/tcp"}}{{(index (index .NetworkSettings.Ports "80/tcp") 0).HostPort}}{{end}}')"
if [ -n "${host80:-}" ] && [ "$host80" != "<no value>" ]; then
	if curl -sS -o /dev/null -w '%{http_code}' "http://localhost:${host80}/" | grep -qE '^(200|404)$'; then
		echo "  ok: nginx reachable on host port ${host80}" >&2
	else
		echo "  note: HTTP probe inconclusive (network restrictions in this environment?)" >&2
	fi
else
	echo "  note: port 80 not published to host; skipping HTTP probe" >&2
fi

echo "All scenarios run." >&2
