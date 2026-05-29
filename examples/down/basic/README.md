# Down: Stop and Remove a Dev Container

`deacon down` honors the `shutdownAction` declared in `devcontainer.json`
(`stopContainer` for single-container configs, `stopCompose` for Compose
configs, or `none` to leave everything running) and adds CLI flags to opt
into stronger teardown: `--remove`, `--volumes`, `--force`, `--all`.

This example uses a single-container config (`shutdownAction:
"stopContainer"`, the default) with a named volume so we can demonstrate
`--volumes` actually removes it.

## Files

- `.devcontainer/devcontainer.json` — image-based config with a named
  volume mount (`deacon-down-demo`) so volume cleanup is observable.

## Scenarios exercised by `exec.sh`

1. **Default stop.** `deacon up` then `deacon down` — container is stopped
   but not removed; the named volume survives.
2. **`--remove`.** `deacon down --remove` removes the stopped container.
   The volume still survives (named volumes are out-of-scope for `--remove`).
3. **`--remove --volumes`.** Brings the container back and tears it down
   along with anonymous volumes attached to it. (Named volumes managed by
   Docker outside deacon must be removed explicitly — `--volumes` only
   cleans anonymous ones per spec.)
4. **`--force`.** Removes the container even when stop would normally
   block (e.g., shutdown took too long).
5. **`--all`.** Includes stale containers matching the workspace labels.
6. **Idempotency.** Running `down` again on an already-removed container
   exits 0.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container

# Stop only (default shutdownAction = stopContainer).
deacon down --workspace-folder .

# Stop + remove the container.
deacon up --workspace-folder . --remove-existing-container
deacon down --workspace-folder . --remove

# Stop + remove + drop anonymous volumes.
deacon up --workspace-folder . --remove-existing-container
deacon down --workspace-folder . --remove --volumes

# Sweep up stale containers matching workspace labels.
deacon down --workspace-folder . --all --remove
```

## Spec references

- `shutdownAction`: <https://containers.dev/implementors/json_reference/>
- Compose teardown (`stopCompose`) is covered by
  `examples/compose/multiservice-down/` and is the natural follow-up to
  this single-container example.
