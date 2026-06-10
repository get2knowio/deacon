# up --auto-forward (dynamic user-space port forwarding)

`deacon up --auto-forward` is a **deacon extension** (modeled on VS Code Dev
Containers): it starts a detached, host-side forwarder that makes container TCP
ports reachable on **loopback** host ports — including `127.0.0.1`-bound servers
that static `-p` publishing cannot reach — and returns control to your shell.

This example's container (`alpine:3.18`) runs a tiny `nc` banner server bound
inside the container and declares `forwardPorts: [3000]`.

## Scenarios (`exec.sh`)

1. **Loopback reach** — `up --auto-forward`, then connect to the reported
   `127.0.0.1:<host-port>` and read the banner the in-container server emits.
   This is the headline capability static `-p` can't provide.
2. **Collision-free multi-container** — bring up a second copy of the same
   config; both serve container port `3000`, and the host-global registry hands
   each a **distinct** host port (the second is remapped).

The script isolates state under a temporary `--user-data-folder` and removes all
containers, detached forwarders, and temp dirs on exit.

```bash
./exec.sh
```

## Notes & limits (v1)

- **Loopback only** (`127.0.0.1`, never `0.0.0.0`/LAN) and **TCP only**.
- **Unix hosts only**; **best-effort** (a forwarding failure warns but `up` still
  succeeds). Privileged container ports (<1024) remap to an unprivileged host
  port — no host root needed.
- Forwarder logs: `<user-data-folder>/forward_daemon_<container_id>.log`.
- Running inside the deacon monorepo, the example passes
  `--mount-workspace-git-root false` so each workspace mounts directly (the
  git-root mount default would otherwise relocate files; not a deacon bug — see
  `CLAUDE.md`).
