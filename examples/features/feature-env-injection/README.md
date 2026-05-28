# Features: Install-Time Env Var Injection

The features spec requires the CLI to inject a set of environment
variables into every feature's `install.sh` so the script can target the
correct user/home without hard-coding paths. The minimum set:

| Var                       | Value                                                            |
|---------------------------|------------------------------------------------------------------|
| `_REMOTE_USER`            | The resolved `remoteUser` (the human's login).                   |
| `_REMOTE_USER_HOME`       | That user's home directory.                                      |
| `_CONTAINER_USER`         | The user the container process itself runs as (`containerUser`). |
| `_CONTAINER_USER_HOME`    | Container-user's home directory.                                  |

This example wires a local feature whose `install.sh` simply records
those four values to `/usr/local/share/feature-env/snapshot`. After
`up`, the snapshot is read back and asserted against the config
(`remoteUser: vscode`, `containerUser: vscode`).

## Files

- `devcontainer.json` — references the local `./capture-env` feature.
- `capture-env/devcontainer-feature.json` — minimal feature metadata.
- `capture-env/install.sh` — captures the four env vars.

## Scenarios exercised by `exec.sh`

1. **All four vars set.** None of the snapshot entries read
   `<unset>`.
2. **Values match config.** `_REMOTE_USER=vscode`,
   `_REMOTE_USER_HOME=/home/vscode`,
   `_CONTAINER_USER=vscode`,
   `_CONTAINER_USER_HOME=/home/vscode`.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /usr/local/share/feature-env/snapshot
```

## Known deacon issues this example surfaces

- [#69](https://github.com/get2knowio/deacon/issues/69) — when this
  example is run via `--config <path>` (e.g. for verification outside
  the standard `.devcontainer/devcontainer.json` layout), local feature
  paths of the form `./feature-X` are misinterpreted as OCI registry
  refs (`registry: "."`).

## Spec references

- Feature install-time env vars:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/features-user-env-variables.md>
- Feature install contract:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-features.md>
