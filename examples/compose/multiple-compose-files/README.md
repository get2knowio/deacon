# Compose: `dockerComposeFile` as Array

The spec allows `dockerComposeFile` to be either a single string or an
array. When an array is provided, the runtime merges the files **in
order** (later files override earlier ones), exactly like
`docker compose -f base.yml -f override.yml`. Most existing examples
use the string form; this one covers the array form explicitly.

The pair here:
- `docker-compose.yml` sets `BASE` and `FINAL=base-wins`.
- `docker-compose.override.yml` adds `OVERRIDE` and re-declares
  `FINAL=override-wins`.

After `up`, the merged service env should contain all three keys with
`FINAL=override-wins`.

## Files

- `devcontainer.json` — `dockerComposeFile` array, single target
  `service: app`, lifecycle hook dumps relevant env vars to
  `/tmp/merged.env`.
- `docker-compose.yml` / `docker-compose.override.yml` — minimal Alpine
  service with different env values.

## Scenarios exercised by `exec.sh`

1. **Both files are parsed.** `BASE=from-base` and
   `OVERRIDE=from-override` are both present inside the container.
2. **Later file wins on conflict.** `FINAL=override-wins`.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /tmp/merged.env
# BASE=from-base
# FINAL=override-wins
# OVERRIDE=from-override
```

## Spec references

- `dockerComposeFile` array form:
  <https://containers.dev/implementors/json_reference/>
- Compose merge order (left-to-right, later wins):
  <https://docs.docker.com/compose/multiple-compose-files/>
