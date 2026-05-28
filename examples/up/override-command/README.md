# Up: `overrideCommand: false`

By default, deacon replaces the image's `CMD`/`ENTRYPOINT` with
`sleep infinity` so the container stays alive while lifecycle hooks run
and the IDE attaches. That's correct for most images, but it suppresses
custom init logic that an image author may have baked into its `CMD`.

`overrideCommand: false` opts out of the override: the container runs
exactly what the image declares. Use this for images that already have a
long-running supervisor (`systemd`, `pid1`, an app server) and rely on
its side effects.

## Files

- `.devcontainer/devcontainer.json` — `overrideCommand: false`, custom
  build with the Dockerfile below.
- `.devcontainer/Dockerfile` — image whose `CMD` drops a marker file at
  `/tmp/image.cmd` before sleeping. The marker is the proof the image's
  command ran.

## Scenarios exercised by `exec.sh`

1. **`overrideCommand: false` honored.** After `deacon up`, the file
   `/tmp/image.cmd` exists inside the container. The image's `CMD` ran.
2. **Compare to default (override: true).** Re-run the same workspace
   with `--override-config` flipping `overrideCommand` back to `true`.
   `/tmp/image.cmd` is now absent — deacon replaced the command.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /tmp/image.cmd   # image-cmd-ran

deacon up --workspace-folder . --remove-existing-container \
	--override-config ./override.true.json
docker exec <cid> ls /tmp/image.cmd   # ENOENT — overridden
```

## Known deacon issues this example surfaces

- [#65](https://github.com/get2knowio/deacon/issues/65) — `--override-config`
  filename validation rejects `override.true.json`.

## Spec references

- `overrideCommand`: <https://containers.dev/implementors/json_reference/>
