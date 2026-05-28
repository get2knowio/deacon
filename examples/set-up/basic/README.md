# Set-Up: Attach to an Existing Container

`deacon set-up` is the "I already have a container, make it a dev container"
workflow. It takes a `--container-id` (required), layers a devcontainer
configuration on top of the container's existing image metadata, runs the
lifecycle hooks, and emits a JSON snapshot of the resulting configuration.

This is what an IDE does when you "Attach to Running Container" — the
container exists, but the lifecycle hooks haven't run yet.

## Files

- `devcontainer.json` — referenced explicitly via `--config`. Sets
  `remoteEnv` and a few lifecycle hooks that drop marker files inside the
  container so we can observe the work `set-up` did.

## Scenarios exercised by `exec.sh`

1. **Start a vanilla container outside deacon.** Plain `docker run` of
   `alpine:3.18 sleep infinity`. No labels, no metadata.
2. **Apply the config.** `deacon set-up --container-id $CID --config
   ./devcontainer.json` runs `onCreate`, `postCreate`, `postStart` against
   the existing container.
3. **Inspect the snapshot.** The command writes the merged configuration as
   JSON on stdout; we pluck `remoteUser` and `remoteEnv.DEACON_SET_UP_DEMO`
   to confirm the layering worked.
4. **Skip lifecycle.** Re-run with `--skip-post-create` to show set-up can
   produce the snapshot without re-executing hooks.

## Manual usage

```sh
CID=$(docker run -d --rm alpine:3.18 sleep infinity)

# Layer a devcontainer.json over the running container.
deacon set-up --container-id "$CID" --config ./devcontainer.json

# JSON-only stdout for parsing.
deacon set-up --container-id "$CID" --config ./devcontainer.json \
	--log-format json 2>/dev/null | jq '.configuration.remoteEnv'

# Snapshot only, no lifecycle execution.
deacon set-up --container-id "$CID" --config ./devcontainer.json --skip-post-create

docker rm -f "$CID"
```

## Spec references

- `set-up` is one of the consumer-side commands defined in the supporting
  tools doc: <https://github.com/devcontainers/spec/blob/main/docs/specs/supporting-tools.md>
- Image-metadata merge (which `set-up` performs against the image's
  `devcontainer.metadata` label):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md#image-metadata>
