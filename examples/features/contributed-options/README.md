# Features: Feature-Contributed Container Options

A Feature's `devcontainer-feature.json` can contribute container-level options
beyond install scripts: `mounts`, `entrypoint`, `init`, `privileged`,
`capAdd`, and `securityOpt`. deacon merges these into the container it creates.
This example verifies four of them actually reach the running container (the
existing `feature-env-injection/` and `feature-contributed-lifecycle/`
canaries cover `containerEnv` and lifecycle hooks respectively).

## The feature (`./probe-feature`)

Declares:
- `mounts` — a named volume mounted at `/contrib-data`
- `entrypoint` — `/usr/local/share/contrib/entrypoint.sh` (created by
  `install.sh`); deacon chains it ahead of the container command, and it ends
  with `exec "$@"`. On start it writes `/tmp/contrib-entrypoint-ran`.
- `init` — request the tini init process
- `capAdd` — `SYS_PTRACE`

## Scenarios exercised by `exec.sh`

1. **Mount** — `docker inspect` shows `/contrib-data` mounted.
2. **Capability** — `HostConfig.CapAdd` contains `SYS_PTRACE`.
3. **Init** — `HostConfig.Init` is `true`.
4. **Entrypoint** — the marker the chained entrypoint writes exists in the
   container.

`exec.sh` removes the named volume on exit.

## Spec references

- Feature contribution properties:
  <https://containers.dev/implementors/features/#devcontainer-feature-json-properties>
