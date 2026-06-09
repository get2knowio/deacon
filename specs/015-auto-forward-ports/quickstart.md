# Quickstart: `deacon up --auto-forward`

Dynamic, user-space port forwarding for a running devcontainer — modeled on VS Code. Reaches `127.0.0.1`-bound servers, auto-detects ports that start later, returns control to your shell, and works across multiple devcontainers at once.

## Reach a loopback-only server (the thing `-p` can't do)

```bash
# A container whose dev server binds 127.0.0.1:3000 inside the container.
deacon up --auto-forward
# -> control returns immediately; stderr prints e.g.:
#    Forwarding container 3000 -> http://127.0.0.1:3000 (web)

curl http://127.0.0.1:3000      # works — even though the server is loopback-only inside
```

Without `--auto-forward`, a `127.0.0.1`-bound container server is unreachable from the host (static `-p` can only reach `0.0.0.0`).

## Auto-detect a port that starts later

```bash
deacon up --auto-forward
deacon exec bash -lc 'npm run dev'   # starts a server on :5173 AFTER up; no exec flag needed
# within ~1-2s, stderr:  Forwarding container 5173 -> http://127.0.0.1:5173
```

The forwarder watches the container's listening sockets, so it catches servers started by `postStart`, the entrypoint, a compose `CMD`, or an interactive `exec` — `exec` is unchanged.

## Run two devcontainers at once (collision-free)

```bash
# Terminal A
deacon up --auto-forward --workspace-folder ~/app-a   # server on container :3000
#    Forwarding container 3000 -> http://127.0.0.1:3000

# Terminal B
deacon up --auto-forward --workspace-folder ~/app-b   # also server on container :3000
#    Forwarding container 3000 -> http://127.0.0.1:3001 (remapped; host 3000 in use)

curl http://127.0.0.1:3000   # app-a
curl http://127.0.0.1:3001   # app-b
```

Host ports are allocated from a host-global registry (`~/.deacon/forwarded_ports.json`); the actual local port is always reported.

## Declared ports

```jsonc
// devcontainer.json
{
  "forwardPorts": [8080, "db:5432"],            // "service:port" targets a compose service
  "portsAttributes": {
    "8080": { "label": "api", "onAutoForward": "notify" },
    "9229": { "onAutoForward": "ignore" }        // never forwarded
  }
}
```

With `--auto-forward`, declared ports are forwarded by the daemon (loopback relay) and are **not** also `-p` published. Declared ports are reserved eagerly at `up` time; undeclared ports are forwarded as they appear.

## Teardown

```bash
deacon down                       # stops the forwarder, releases its host ports, no orphans
deacon up --remove-existing-container --auto-forward   # reaps the old forwarder first
# If the container is removed out-of-band (docker rm -f), the forwarder self-exits and cleans up.
```

## Notes & limits (v1)

- **Loopback only**: forwards bind `127.0.0.1` on the host (never `0.0.0.0`/LAN).
- **TCP only**.
- **Best-effort**: if forwarding can't start, you get a clear warning but `up` still succeeds and the container runs.
- **No root needed**: privileged container ports (e.g. 80) are remapped to an unprivileged host port and reported.
- **Unix** host detach in v1 (Windows tracked separately).
- Forwarder logs: `~/.deacon/forward_daemon_<container_id>.log`.

## Verify (acceptance smoke)

```bash
# 1. loopback reach
deacon up --auto-forward && curl -fsS http://127.0.0.1:3000 >/dev/null && echo OK

# 2. registry has the entry
cat ~/.deacon/forwarded_ports.json | jq '.entries[].host_port'

# 3. clean teardown releases the port
deacon down && cat ~/.deacon/forwarded_ports.json | jq '.entries | length'   # -> 0 for that container
```
