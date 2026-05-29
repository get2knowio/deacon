# Compose: `runServices` Selectivity

`runServices` lists the Compose services deacon should start/stop alongside the
primary `service`. When set, services *not* in `service` ∪ `runServices` should
stay down. This canary verifies that selectivity.

## Files

- `docker-compose.yml` — three `alpine` services: `app`, `worker`, `idle`
  (each labeled `canary.svc=<name>`).
- `.devcontainer.json` — `service: app`, `runServices: ["worker"]`.

## Scenario exercised by `exec.sh`

After `up`: `app` (primary) and `worker` (runServices) are running, while
`idle` — listed in the compose file but not selected — stays down.

## Spec references

- `runServices`: <https://containers.dev/implementors/json_reference/>
