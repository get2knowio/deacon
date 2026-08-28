#!/usr/bin/env bash
# Start podman's API socket, which `podman compose` needs and nothing else does.
#
# `podman compose` is a thin shim: it execs an external provider (docker-compose
# on this image) with DOCKER_HOST pointed at podman's own socket —
# MEASURED by pointing `compose_providers` at a script that dumps its
# environment, podman exports
#   DOCKER_HOST=unix:///run/user/<uid>/podman/podman.sock
#   DOCKER_BUILDKIT=0
# and blanks DOCKER_CONFIG. With nothing listening on that path every compose
# call fails with `Cannot connect to the Docker daemon`, which is what the whole
# compose half of the suite does without this.
#
# Normally `podman.socket` would be socket-activated by the user's systemd
# instance; this container has no systemd, so run the service directly.
# Idempotent: exits quietly if the socket is already answering.
set -euo pipefail

uid="$(id -u)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/${uid}}"
sock="${XDG_RUNTIME_DIR}/podman/podman.sock"

if [ ! -d "${XDG_RUNTIME_DIR}" ]; then
  sudo mkdir -p "${XDG_RUNTIME_DIR}"
  sudo chown "${uid}:$(id -g)" "${XDG_RUNTIME_DIR}"
  sudo chmod 700 "${XDG_RUNTIME_DIR}"
fi

if curl -s --max-time 5 --unix-socket "${sock}" http://d/v1.41/_ping >/dev/null 2>&1; then
  echo "[podman-socket] already listening at ${sock}"
  exit 0
fi

mkdir -p "$(dirname "${sock}")"
rm -f "${sock}"
# --time=0 disables the idle exit, so the socket outlives the shell that started it.
setsid podman system service --time=0 "unix://${sock}" \
  >"${XDG_RUNTIME_DIR}/podman-service.log" 2>&1 < /dev/null &
disown || true

for _ in $(seq 1 25); do
  if curl -s --max-time 2 --unix-socket "${sock}" http://d/v1.41/_ping >/dev/null 2>&1; then
    echo "[podman-socket] listening at ${sock}"
    exit 0
  fi
  sleep 0.4
done

echo "[podman-socket] socket did not come up; see ${XDG_RUNTIME_DIR}/podman-service.log" >&2
exit 1
