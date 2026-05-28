# Features: `overrideFeatureInstallOrder`

The spec lets a configuration **force a specific feature install order**
that supersedes the default (which would otherwise come from
`dependsOn` / `installsAfter` plus alphabetical tie-breaking). The
override is an array of feature IDs in the order they should run.

This example wires three independent local features (`alpha`, `bravo`,
`charlie`) — no dependencies, so the default would be alphabetical:
`alpha → bravo → charlie`. The `overrideFeatureInstallOrder` forces
`charlie → alpha → bravo`. Each feature appends its name to
`/tmp/feature-order/log`, making the actual order observable.

## Files

- `devcontainer.json` — declares all three local features and the
  override order.
- `feature-alpha/`, `feature-bravo/`, `feature-charlie/` — each has a
  minimal `devcontainer-feature.json` and an `install.sh` that appends
  its name to the marker file.

## Scenarios exercised by `exec.sh`

1. **Override applied.** After `deacon up`,
   `/tmp/feature-order/log` reads `charlie / alpha / bravo` (the override
   order), not `alpha / bravo / charlie` (the alphabetical default).

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /tmp/feature-order/log
# charlie
# alpha
# bravo
```

## Known deacon issues this example surfaces

- [#69](https://github.com/get2knowio/deacon/issues/69) — when
  `devcontainer.json` is loaded via `--config <path>`, local feature
  paths of the form `./feature-X` are misinterpreted as OCI registry
  refs (`registry: "."`). This example uses local features and so hits
  the bug when invoked outside the standard `.devcontainer/devcontainer.json`
  discovery location.

## Spec references

- `overrideFeatureInstallOrder`:
  <https://containers.dev/implementors/json_reference/>
- Feature installation order semantics (default and override):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-features.md>
