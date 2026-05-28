# Up: `userEnvProbe` Matrix (Lifecycle Side)

`userEnvProbe` controls **how the container's shell environment is
collected** before lifecycle hooks (and later `deacon exec`) run. The spec
defines four values, and they materially change which login/interactive
init files contribute to `PATH` and any custom env exported from
`~/.bashrc` or `~/.profile`.

| Value                    | Login (`-l`) | Interactive (`-i`) |
|--------------------------|:------------:|:------------------:|
| `none`                   |      ✗       |         ✗          |
| `interactiveShell`       |      ✗       |         ✓          |
| `loginShell`             |      ✓       |         ✗          |
| `loginInteractiveShell`  |      ✓       |         ✓          |

This example covers the **up-side**: each variant drives a different
`postCreateCommand` environment, captured to `/tmp/probe.path` and
`/tmp/probe.var` inside the container. (`examples/exec/user-env-probe-modes/`
already covers the `deacon exec --default-user-env-probe` flag.)

## Files

- `.devcontainer/devcontainer.json` — base config, `userEnvProbe:
  loginInteractiveShell`. The `postCreateCommand` captures `$PATH` and a
  variable `PROBE_VAR` that is set only by `~/.bashrc` in the base image.
- `override.interactive.json`, `override.login.json`,
  `override.none.json` — apply via `--override-config` to flip only
  the probe mode.

## Scenarios exercised by `exec.sh`

For each of the four modes, `exec.sh`:

1. Brings the container up with the matching config / override.
2. Reads `/tmp/probe.path` and `/tmp/probe.var` out of the container.
3. Prints the PATH and PROBE_VAR for that mode side-by-side.

Expected differences:

- `none` produces the bare container PATH and `PROBE_VAR=<unset>`.
- `loginShell` adds entries from `/etc/profile` / `~/.profile` but skips
  `~/.bashrc`, so `PROBE_VAR` is still unset.
- `interactiveShell` sources `~/.bashrc` (giving `PROBE_VAR=set`) but
  may miss login-only PATH additions.
- `loginInteractiveShell` (default) gets both.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container

# Flip to interactive-only without editing the base config:
deacon up --workspace-folder . --remove-existing-container \
	--override-config ./override.interactive.json
```

## Known deacon issues this example surfaces

- [#65](https://github.com/get2knowio/deacon/issues/65) — `--override-config`
  filename validation rejects `override.interactive.json` /
  `override.login.json` / `override.none.json`.

## Spec references

- `userEnvProbe`: <https://containers.dev/implementors/json_reference/>
- Devcontainer reference (env probe):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
