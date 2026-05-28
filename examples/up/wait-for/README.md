# Up: `waitFor` Phase Selection

`waitFor` selects the lifecycle phase at which `deacon up` (and any
consumer using `--skip-non-blocking-commands`) considers the container
"ready". Allowed values: `onCreateCommand`, `updateContentCommand`
(default), `postCreateCommand`. The phases that come after `waitFor` are
treated as non-blocking and can be skipped to shorten the critical path.

This example pairs `waitFor` with `--skip-non-blocking-commands` to make
the cutoff observable in `/tmp/lifecycle.log` inside the container.

## Files

- `.devcontainer/devcontainer.json` — defines all five lifecycle hooks
  appending to `/tmp/lifecycle.log`. No explicit `waitFor` so the default
  (`updateContentCommand`) applies in scenario 1.
- `override.onCreate.json`, `override.postCreate.json` — flip only
  `waitFor` via `--override-config` for scenarios 2 and 3.

## Scenarios exercised by `exec.sh`

1. **Default (`updateContentCommand`)** + `--skip-non-blocking-commands`.
   The log should contain `onCreate` and `updateContent`, but NOT
   `postCreate`, `postStart`, or `postAttach`.
2. **`waitFor: onCreateCommand`** + `--skip-non-blocking-commands`. The
   log should contain only `onCreate`.
3. **`waitFor: postCreateCommand`** + `--skip-non-blocking-commands`. The
   log should contain `onCreate`, `updateContent`, and `postCreate`, but
   not the post-start / post-attach entries.
4. **Default `waitFor` without the skip flag.** All five phases run; the
   log contains everything.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container \
	--skip-non-blocking-commands

deacon up --workspace-folder . --remove-existing-container \
	--override-config ./override.postCreate.json \
	--skip-non-blocking-commands
```

## Known deacon issues this example surfaces

- [#65](https://github.com/get2knowio/deacon/issues/65) — `--override-config`
  filename validation rejects `override.onCreate.json` /
  `override.postCreate.json`. The upstream `@devcontainers/cli` accepts any
  filename.

## Spec references

- `waitFor` definition and enum: <https://containers.dev/implementors/json_reference/>
- Lifecycle phase ordering:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md#lifecycle-scripts>
