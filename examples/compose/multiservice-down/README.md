# Compose: Multi-Service Teardown (`down` / `stopCompose`)

`down/basic/` covers single-container teardown; this is its Compose
counterpart. It brings up a two-service Compose project (`app` + `db`) and
exercises `deacon down` against it, including the `shutdownAction:
stopCompose` default and the `--remove` / `--volumes` flags.

## Files

- `docker-compose.yml` — two `alpine` services (`app`, `db`); `db` has a named
  volume `msdown-data`. Both carry the label `canary.group=msdown` so the
  script can find them without knowing deacon's derived project name.
- `.devcontainer.json` — `dockerComposeFile` + `service: app`,
  `runServices: ["db"]` (so both services come up — deacon starts the primary
  `service` plus `runServices`), `shutdownAction: stopCompose`.

## Scenarios exercised by `exec.sh`

1. **Up** starts both services (2 running).
2. **`down`** (default `stopCompose`) stops both but leaves them present.
3. **`down --remove`** deletes the project's containers.
4. **`down --remove --volumes`** also drops the named volume.

## Spec references

- `shutdownAction` (`stopCompose`):
  <https://containers.dev/implementors/json_reference/>
