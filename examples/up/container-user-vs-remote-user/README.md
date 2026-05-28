# Up: `containerUser` vs `remoteUser`

These two properties **mean different things** and the distinction trips
people up:

- **`containerUser`** is the user the *container process itself* runs as
  (i.e., what `--user` passes to `docker run`). When unset, it defaults to
  whatever `USER` the image declares.
- **`remoteUser`** is the user that lifecycle hooks (`postCreateCommand`,
  …) and later `deacon exec` invocations run as inside that container.
  When unset, it defaults to `containerUser`.

So with `containerUser: "root"` and `remoteUser: "vscode"`:

- `docker exec <cid>` (raw) shows the container PID 1 as **root**.
- `deacon exec` and lifecycle hooks show the process running as **vscode**.

The base image used here (`mcr.microsoft.com/devcontainers/base:debian`)
ships with both users available, which is what makes the demonstration
work cleanly.

## Files

- `.devcontainer/devcontainer.json` — `containerUser: root`, `remoteUser:
  vscode`. The `postCreateCommand` records `id -un` to
  `/tmp/postcreate.user` so we can verify which user lifecycle ran as.

## Scenarios exercised by `exec.sh`

1. **PID-1 user.** Inspect the running container with raw `docker exec`
   (no user override). Expected: `root`.
2. **Lifecycle user.** Read `/tmp/postcreate.user` produced by
   `postCreateCommand`. Expected: `vscode`.
3. **`deacon exec` user.** Run `deacon exec id -un`. Expected: `vscode`.
4. **`--user-id` override on `deacon exec`** still gets vscode by default;
   the spec lets remoteUser be overridden by an explicit flag at the
   exec call site if needed (not exercised here — see
   `examples/exec/remote-user-execution/`).

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> id -un          # root (containerUser)
deacon exec --workspace-folder . id -un   # vscode (remoteUser)
```

## Spec references

- `containerUser` and `remoteUser`:
  <https://containers.dev/implementors/json_reference/>
