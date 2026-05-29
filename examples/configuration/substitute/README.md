# Configuration: `config substitute`

`deacon config substitute` loads a `devcontainer.json`, applies `${...}`
variable substitution, and prints the resolved configuration — without
touching Docker. Useful for debugging what a config resolves to before an
`up`.

## Files

- `.devcontainer.json` — uses `${localEnv:CANARY_TOKEN}` and
  `${localWorkspaceFolderBasename}` in the name, `workspaceFolder`, and
  `containerEnv`.

## Scenarios exercised by `exec.sh`

1. **Substitution** — with `CANARY_TOKEN=zzz`, the output contains `subst-zzz`,
   `hi-zzz`, and `/wf/substitute` (this directory's basename).
2. **`--dry-run`** previews successfully.
3. **Valid JSON** (when `python3` is present) — the output parses and the
   resolved config is under `.configuration` with `name == "subst-zzz"`.

> `config substitute` resolves host-side variables (`localEnv`,
> `localWorkspaceFolder*`). `${containerWorkspaceFolder}` is only known once a
> container exists, so it is left for the `up`/`read-configuration` flow.

## Spec references

- Variable interpolation:
  <https://containers.dev/implementors/json_reference/#variables-in-devcontainerjson>
