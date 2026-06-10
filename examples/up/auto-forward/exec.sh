#!/usr/bin/env bash
# Canary for `deacon up --auto-forward` (deacon extension: dynamic, user-space
# port forwarding). Demonstrates:
#   1. Loopback reach — a 127.0.0.1-bound container server reachable on the host
#      (something static `-p` cannot do).
#   2. Multi-container collision-free allocation — two devcontainers both serving
#      container port 3000 get two distinct host ports from the host-global
#      registry.
# All resources (containers, detached forwarders, temp dirs) are cleaned up.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEACON_BIN="${DEACON_BIN:-deacon}"

# Isolated host-global registry + markers so the canary never touches ~/.deacon.
UDF="$(mktemp -d)"
# Second workspace for the multi-container scenario (a copy of this config).
WS2="$(mktemp -d)"
cp -r "$SCRIPT_DIR/.devcontainer" "$WS2/.devcontainer"

run() {
	echo "+ $*" >&2
	"$@"
}

# Inside the monorepo, `up` mounts the git root unless told otherwise (the
# intended --mount-workspace-git-root default); pass false so each example
# workspace mounts directly. Not a deacon bug — see CLAUDE.md.
UP_FLAGS=(--auto-forward --mount-workspace-git-root false --remove-existing-container)

cleanup() {
	# Kill any forwarders recorded in the isolated registry's markers, remove
	# the example's containers (matched by the shared config name label), and
	# drop the temp dirs.
	for pidfile in "$UDF"/forward_daemon_*.pid; do
		[ -f "$pidfile" ] || continue
		pid="$(sed -n 's/.*"pid"[: ]*\([0-9]*\).*/\1/p' "$pidfile" | head -n1 || true)"
		[ -n "${pid:-}" ] && kill "$pid" 2>/dev/null || true
	done
	docker ps -aq --filter "label=devcontainer.name=auto-forward" | xargs -r docker rm -f >/dev/null 2>&1 || true
	rm -rf "$UDF" "$WS2" "$SCRIPT_DIR/.devcontainer-state"
}
trap cleanup EXIT

# Fetch from a loopback host port via bash /dev/tcp (no curl/nc needed on host).
fetch() {
	local port="$1" out=""
	for _ in $(seq 1 20); do
		# The busybox `nc` server may not half-close, so `cat` is killed by
		# `timeout` (non-zero exit) *after* receiving the banner — gate on the
		# captured output, not the exit code. `|| true` keeps set -e happy.
		out="$(timeout 4 bash -c "exec 3<>/dev/tcp/127.0.0.1/$port; printf 'GET / HTTP/1.0\r\n\r\n' >&3; cat <&3" 2>/dev/null || true)"
		if [ -n "$out" ]; then
			printf '%s' "$out"
			return 0
		fi
		sleep 0.5
	done
	return 1
}

wait_for_registry_entries() {
	local want="$1" n
	for _ in $(seq 1 30); do
		# `|| n=0` keeps set -e happy when the file/keys are not present yet.
		n="$(grep -c '"host_port"' "$UDF/forwarded_ports.json" 2>/dev/null)" || n=0
		[ "${n:-0}" -ge "$want" ] && return 0
		sleep 0.5
	done
	return 1
}

echo "== Scenario 1: reach a loopback-only server through the forwarder ==" >&2
run "$DEACON_BIN" --user-data-folder "$UDF" up --workspace-folder "$SCRIPT_DIR" "${UP_FLAGS[@]}" >/dev/null
wait_for_registry_entries 1
HP1="$(sed -n 's/.*"host_port"[: ]*\([0-9]*\).*/\1/p' "$UDF/forwarded_ports.json" | head -n1)"
echo "  forwarded container 3000 -> host 127.0.0.1:${HP1}" >&2
body="$(fetch "$HP1")" || { echo "  FAIL: loopback port not reachable" >&2; exit 1; }
echo "$body" | grep -q "deacon-forward-ok" || { echo "  FAIL: unexpected response: $body" >&2; exit 1; }
echo "  ok: loopback-only container server reachable on the host" >&2

echo "== Scenario 2: a second devcontainer gets a distinct host port ==" >&2
run "$DEACON_BIN" --user-data-folder "$UDF" up --workspace-folder "$WS2" "${UP_FLAGS[@]}" >/dev/null
wait_for_registry_entries 2
mapfile -t PORTS < <(sed -n 's/.*"host_port"[: ]*\([0-9]*\).*/\1/p' "$UDF/forwarded_ports.json" | sort -u)
echo "  registry host ports: ${PORTS[*]}" >&2
if [ "${#PORTS[@]}" -ge 2 ]; then
	echo "  ok: two devcontainers serving container 3000 got distinct host ports (collision-free)" >&2
else
	echo "  FAIL: expected two distinct host ports, got: ${PORTS[*]}" >&2
	exit 1
fi

echo "All scenarios run." >&2
