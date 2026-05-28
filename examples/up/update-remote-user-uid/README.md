# Up: `updateRemoteUserUID`

On Linux, files written from inside a container are owned by the
container's UID/GID. If the container ships with UID 5000 and the host
user is UID 1000, bind-mounted workspace files end up owned by `5000`
back on the host — which is a permission nightmare.

`updateRemoteUserUID: true` (the spec's default on Linux) tells the
runtime to **re-stamp `remoteUser`'s UID/GID inside the container to
match the host user's** before lifecycle hooks run. Files written from
inside the container then land at the right ownership on the host.

This example builds an image that creates `devuser` with UID/GID 5000,
then runs with and without the UID sync to make the difference visible.

## Files

- `.devcontainer/Dockerfile` — creates `devuser` with UID 5000 / GID 5000.
- `.devcontainer/devcontainer.json` — `updateRemoteUserUID: true`,
  records `id -u`, `id -g`, and the workspace mount's owning UID.
- `override.disable.json` — flips the flag to `false` for the contrast
  scenario.

## Scenarios exercised by `exec.sh`

1. **With `updateRemoteUserUID: true`**, `id -u` inside the container
   equals the host user's UID — not the image's 5000.
2. **With `updateRemoteUserUID: false`** (via override), `id -u`
   stays at 5000 and the workspace's stat ownership is unchanged from
   the image.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> id -u   # should equal `id -u` on the host

deacon up --workspace-folder . --remove-existing-container \
	--override-config ./override.disable.json
docker exec <cid> id -u   # 5000 (unchanged from the image)
```

## Known deacon issues this example surfaces

- [#65](https://github.com/get2knowio/deacon/issues/65) — `--override-config`
  filename validation rejects `override.disable.json`.

## Notes

- This is Linux-only semantics; on macOS / Windows the spec leaves the
  flag's effect undefined because the Docker VM handles UID translation
  already.
- The `--update-remote-user-uid-default` CLI flag sets the default when
  the config doesn't specify; explicit config wins.

## Spec references

- `updateRemoteUserUID`: <https://containers.dev/implementors/json_reference/>
