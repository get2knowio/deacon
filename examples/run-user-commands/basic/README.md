# Run-User-Commands: Basic

Re-execute lifecycle hooks against an already-running container without
recreating it. This is the "I just edited my `postCreateCommand`, re-run it"
workflow. The lifecycle phases the spec defines (`onCreate`, `updateContent`,
`postCreate`, `postStart`, `postAttach`) all run again here.

## Files

- `.devcontainer/devcontainer.json` — every lifecycle hook drops a marker file
  under `/tmp/` so we can observe which phases ran.

## Scenarios exercised by `exec.sh`

1. **Full re-run.** `deacon up --skip-post-create` creates the container with
   the lifecycle hooks suppressed; the marker files are absent. Then
   `deacon run-user-commands` runs *all* hooks and the markers appear.

2. **Prebuild mode** (`--prebuild`). Stops after `updateContentCommand`. The
   `postCreate`, `postStart`, and `postAttach` markers stay from step 1 (or
   are absent on a fresh container) — only `onCreate` and `updateContent`
   are re-driven.

3. **Skip non-blocking commands** (`--skip-non-blocking-commands`). Stops
   after the configured `waitFor` phase (default `updateContent`); the
   `postStart` and `postAttach` hooks are not invoked.

4. **`--container-id` targeting.** The same re-run, but selected by container
   ID instead of workspace folder — the form used by IDE attach workflows.

## Manual usage

```sh
# Start with lifecycle suppressed so we can drive it manually below.
deacon up --workspace-folder . --remove-existing-container --skip-post-create

# Run every phase.
deacon run-user-commands --workspace-folder .

# Stop after updateContentCommand (e.g., for prebuild image layers).
deacon run-user-commands --workspace-folder . --prebuild

# Skip postStart + postAttach (faster iteration).
deacon run-user-commands --workspace-folder . --skip-non-blocking-commands

# Target by container ID (IDE-attach pattern).
CID=$(docker ps --filter label=devcontainer.local_folder="$PWD" --format '{{.ID}}')
deacon run-user-commands --container-id "$CID"
```

## Spec references

- Lifecycle phases and ordering: <https://containers.dev/implementors/spec/#lifecycle-scripts>
- `waitFor` semantics: <https://containers.dev/implementors/json_reference/>
