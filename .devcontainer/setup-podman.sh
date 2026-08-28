#!/usr/bin/env bash
# Make rootless podman work inside this devcontainer, so the Podman CI lane
# (`DEACON_CONTAINER_RUNTIME=podman`) can be reproduced locally instead of only
# in CI. Idempotent: safe to re-run.
#
# Three things have to be true, and each was MEASURED to be false by default here.
#
# 1. SUBORDINATE IDs THAT FIT THE AMBIENT USER NAMESPACE. This container already
#    runs inside a userns — `/proc/self/uid_map` reads `0 100000 65536`, so only
#    ids 0..65535 exist for us. The image ships `vscode:100000:65536`, which
#    names ids this namespace cannot represent, and `newuidmap` fails with
#    `write to uid_map failed: Operation not permitted`. Podman additionally
#    REFUSES any single range containing the user's own uid, so the allocation
#    has to straddle it as two ranges. Getting the size right matters beyond
#    starting a container: a range that stops short of 65534 cannot map `nobody`,
#    and Feature installs then die inside apt with
#    `setgroups 65534 failed - setgroups (22: Invalid argument)`.
#
# 2. A STORAGE DRIVER THE KERNEL WILL ACTUALLY MOUNT. `overlay` rootless goes
#    through fuse-overlayfs, which needs `/dev/fuse`. The kernel module is loaded
#    (`fuse` is in /proc/filesystems) but the device node is absent unless the
#    devcontainer requests it, and `mknod` is not permitted from inside. So:
#    overlay when the node is there, `vfs` otherwise. vfs is slower and correct.
#
# 3. AN API SOCKET, for compose only. `podman compose` shells out to an external
#    provider (docker-compose here) and points it at podman by exporting
#    DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock. With no socket
#    listening, every compose call fails with `Cannot connect to the Docker
#    daemon`. There is no systemd in this container to socket-activate it, so
#    `start-podman-socket.sh` runs it directly; postStartCommand invokes that.
set -euo pipefail

log() { printf '[setup-podman] %s\n' "$*"; }

# ---------------------------------------------------------------- packages ---
# podman pulls most of these in already; naming them keeps the set explicit and
# survives a base-image change. crun is podman's preferred OCI runtime rootless.
if ! command -v podman >/dev/null 2>&1 || ! command -v fuse-overlayfs >/dev/null 2>&1; then
  log "installing podman and its rootless dependencies"
  sudo apt-get update -qq
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    podman uidmap fuse-overlayfs slirp4netns passt catatonit crun >/dev/null
else
  log "podman already present ($(podman --version))"
fi

# ------------------------------------------------------- subuid  /  subgid ---
# First line of uid_map is `<inside_start> <outside_start> <count>`. An
# unremapped host reads `0 0 4294967295`; anything smaller means we are nested.
read -r _inside _outside ambient_count < /proc/self/uid_map
me="$(id -un)"
my_uid="$(id -u)"

if [ "${ambient_count}" -ge 4294967295 ]; then
  # Not in a userns: the conventional allocation is available and is what other
  # tooling expects to see.
  ranges="${me}:100000:65536"
  log "no ambient userns; using the standard 100000:65536 allocation"
else
  # Nested. Use every id the ambient namespace owns except our own uid, as two
  # ranges around it. Podman concatenates them, so container uid N maps to the
  # Nth delegated id and `nobody` (65534) stays representable.
  lo_count=$(( my_uid - 1 ))
  hi_start=$(( my_uid + 1 ))
  hi_count=$(( ambient_count - my_uid - 1 ))
  ranges="${me}:1:${lo_count}
${me}:${hi_start}:${hi_count}"
  log "ambient userns owns ${ambient_count} ids; delegating 1:${lo_count} and ${hi_start}:${hi_count}"
fi

for f in /etc/subuid /etc/subgid; do
  if [ "$(cat "$f" 2>/dev/null || true)" != "${ranges}" ]; then
    printf '%s\n' "${ranges}" | sudo tee "$f" >/dev/null
    log "wrote $f"
  fi
done

# ------------------------------------------------------ XDG_RUNTIME_DIR ------
# Podman keeps its rootless state and its API socket here. Without systemd
# nothing creates it.
runtime_dir="/run/user/${my_uid}"
if [ ! -d "${runtime_dir}" ]; then
  sudo mkdir -p "${runtime_dir}"
  sudo chown "${my_uid}:$(id -g)" "${runtime_dir}"
  sudo chmod 700 "${runtime_dir}"
  log "created ${runtime_dir}"
fi

# ------------------------------------------------------------- storage ------
mkdir -p "${HOME}/.config/containers"
if [ -e /dev/fuse ]; then
  driver=overlay
  extra='mount_program = "/usr/bin/fuse-overlayfs"'
  log "/dev/fuse present; using overlay via fuse-overlayfs"
else
  driver=vfs
  extra=''
  log "/dev/fuse absent; falling back to vfs (slower, but it mounts)"
  log "  add --device=/dev/fuse to runArgs and rebuild to get overlay"
fi
cat > "${HOME}/.config/containers/storage.conf" <<EOF
# Written by .devcontainer/setup-podman.sh — see that script for why.
[storage]
driver = "${driver}"

[storage.options.overlay]
${extra}
EOF

# Storage written under a previous id allocation is owned by ids the new mapping
# cannot express; podman reports "potentially insufficient UIDs or GIDs" on the
# next container create. Re-home the graph root to the current mapping.
if [ -d "${HOME}/.local/share/containers/storage" ]; then
  log "reconciling existing storage with the current id mapping"
  XDG_RUNTIME_DIR="${runtime_dir}" podman system migrate >/dev/null 2>&1 || true
fi

log "done. Verify with: podman run --rm docker.io/library/alpine:3.19 echo ok"
