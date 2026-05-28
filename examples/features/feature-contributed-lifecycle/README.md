# Features: Feature-Contributed Lifecycle Commands

Per spec, a feature's `devcontainer-feature.json` may declare its own
lifecycle hooks (`onCreateCommand`, `updateContentCommand`,
`postCreateCommand`, `postStartCommand`, `postAttachCommand`). These are
NOT replaced by the user's `devcontainer.json` — instead they're
**unioned** with the user's. Every contributing feature's hook runs in
addition to the user's, in a deterministic order that respects the
feature install order.

This example wires two local features that each contribute lifecycle
hooks alongside a user-defined `postCreateCommand`. After `up`, the
`/tmp/lifecycle.log` inside the container should contain entries from
all three sources.

## Files

- `devcontainer.json` — references both local features plus a user
  `postCreateCommand`.
- `monitor/` — feature contributing `postCreateCommand` and
  `postStartCommand`.
- `tooling/` — feature contributing only `postCreateCommand`.

## Scenarios exercised by `exec.sh`

1. **Union, not replacement.** `/tmp/lifecycle.log` contains
   `user-postcreate`, `monitor-postcreate`, and `tooling-postcreate`.
2. **Distinct phases preserved.** The log contains
   `monitor-poststart` from `monitor`'s `postStartCommand`, fired after
   the postCreate phase completes.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /tmp/lifecycle.log
```

## Known deacon issues this example surfaces

- [#69](https://github.com/get2knowio/deacon/issues/69) — when this
  example is run via `--config <path>` (e.g. for verification outside
  the standard `.devcontainer/devcontainer.json` layout), local feature
  paths of the form `./feature-X` are misinterpreted as OCI registry
  refs (`registry: "."`).

## Spec references

- Features contributing lifecycle scripts:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/features-contribute-lifecycle-scripts.md>
- Feature metadata reference:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-features.md>
