# Up: Custom `workspaceMount`

By default, deacon mounts the local workspace folder at
`/workspaces/<basename>` (or `/workspace`, depending on the base image's
convention). The spec lets you override that with `workspaceMount`, a
docker-style mount string that takes `source`, `target`, `type`, and
optional `consistency` parameters.

This example moves the workspace to `/srv/app` and uses `cached`
consistency for macOS-friendly performance. Both knobs are observable in
`docker inspect` and in the lifecycle hook's `pwd`.

## Files

- `.devcontainer/devcontainer.json` — `workspaceMount` overridden, with
  matching `workspaceFolder: "/srv/app"`. `postCreateCommand` records
  `pwd` and a directory listing.

## Scenarios exercised by `exec.sh`

1. **`workspaceFolder` honored.** `postCreateCommand` ran from
   `/srv/app`, captured in `/tmp/pwd`.
2. **Mount target visible in `docker inspect`.** The mount appears with
   `Destination: /srv/app` and the configured consistency.
3. **Files reachable.** `/tmp/listing` contains the workspace contents
   (this README, the `.devcontainer/` dir, etc.) — proves the bind
   mount actually wires the local files in.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container

# Confirm the bind mount points at /srv/app.
docker inspect <cid> --format '{{json .Mounts}}' | jq '.[] | select(.Destination=="/srv/app")'

# Lifecycle hook ran inside /srv/app.
docker exec <cid> cat /tmp/pwd        # /srv/app
docker exec <cid> cat /tmp/listing    # README.md, .devcontainer, ...
```

## Spec references

- `workspaceMount` / `workspaceFolder`:
  <https://containers.dev/implementors/json_reference/>
- Variable substitution (`${localWorkspaceFolder}`):
  <https://containers.dev/implementors/spec/>
