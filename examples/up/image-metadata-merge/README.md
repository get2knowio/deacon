# Up: Image-Metadata Merge (`devcontainer.metadata` label)

The spec lets a base image **carry DevContainer configuration baked in**
as a JSON array on the `devcontainer.metadata` Docker LABEL. When a
user's `devcontainer.json` builds on top of that image, the CLI is
required to merge the two layers (image metadata < user config) before
producing the resolved configuration.

This example builds a tiny Alpine image that ships a
`devcontainer.metadata` label carrying `containerEnv.IMAGE_LAYER` and
`containerEnv.MERGED_LAYER`. The user's `devcontainer.json` adds
`containerEnv.CONFIG_LAYER` and re-declares `MERGED_LAYER` so we can
prove the user-side value wins on conflict.

## Files

- `.devcontainer/Dockerfile` — sets the `devcontainer.metadata` LABEL.
- `.devcontainer/devcontainer.json` — adds its own `containerEnv` keys,
  `postCreateCommand` dumps the merged environment to `/tmp/merged.env`.

## Scenarios exercised by `exec.sh`

1. **Both layers contribute.** `/tmp/merged.env` includes both
   `IMAGE_LAYER=from-image-label` (image-only) and
   `CONFIG_LAYER=from-devcontainer-json` (config-only).
2. **User config wins on conflict.** `MERGED_LAYER` is set differently
   in both layers; the resolved value is the user-config one.
3. **`--include-merged-configuration` surfaces the merge.** The
   resolved JSON exposes `mergedConfiguration.containerEnv` containing
   all three keys.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /tmp/merged.env

deacon read-configuration --workspace-folder . --include-merged-configuration \
	| jq '.mergedConfiguration.containerEnv'
```

## Known deacon issues this example surfaces

- [#70](https://github.com/get2knowio/deacon/issues/70) — deacon does not
  merge the image's `devcontainer.metadata` LABEL into the resolved
  configuration. The example asserts `IMAGE_LAYER` (from the label) is
  visible in the container's env after `deacon up`, which fails today.

## Spec references

- Image metadata merge:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md#image-metadata>
- LABEL contract (`devcontainer.metadata`):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
