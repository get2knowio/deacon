# Up: Security Options (`init`, `capAdd`, `securityOpt`, `privileged`)

DevContainer configuration has four top-level security knobs that the
spec hands directly to the runtime (Docker / Podman) when creating the
container:

- **`init: true`** runs an init process (e.g., `tini`) as PID 1 to reap
  zombie children. Equivalent to `docker run --init`.
- **`privileged: true`** drops the default container capability set in
  favor of full host privilege. Use sparingly; this example keeps it
  `false` and demonstrates the safer alternative via `capAdd`.
- **`capAdd: [...]`** adds specific Linux capabilities (e.g., `SYS_PTRACE`
  so `strace`/`gdb` can attach to other processes in the container).
- **`securityOpt: [...]`** passes through `--security-opt` values
  (`seccomp`, `apparmor`, `no-new-privileges`, etc.).

The example combines all four (with `privileged: false`) and verifies the
runtime accepted them by inspecting the live container.

## Files

- `.devcontainer/devcontainer.json` — `init: true`, `capAdd: [SYS_PTRACE]`,
  `securityOpt: [seccomp=unconfined]`. `postCreateCommand` installs
  `strace` so we can exercise `SYS_PTRACE` end-to-end.

## Scenarios exercised by `exec.sh`

1. **`init` honored.** PID 1 inside the container is *not* the
   `postCreateCommand`'s sleep — it's the runtime's init process
   (`/sbin/docker-init` or `tini`). Verified via `docker exec <cid> cat
   /proc/1/comm`.
2. **`capAdd: SYS_PTRACE` works.** Run `strace -p 1 -e none` against
   PID 1 from inside the container. Without `SYS_PTRACE` this fails with
   `EPERM`; with it, strace attaches.
3. **`securityOpt: seccomp=unconfined` propagated.** Inspect the
   container with `docker inspect` and confirm `seccomp=unconfined`
   appears under `HostConfig.SecurityOpt`.
4. **`privileged: false` honored.** `docker inspect` shows
   `HostConfig.Privileged == false`.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container

# PID 1 should be an init process.
docker exec <cid> cat /proc/1/comm

# Attach strace to PID 1 (works only with SYS_PTRACE).
docker exec <cid> strace -p 1 -e none -o /dev/null
```

## Spec references

- Security fields: <https://containers.dev/implementors/json_reference/>
  (search for `init`, `privileged`, `capAdd`, `securityOpt`).
