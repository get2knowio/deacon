# Doctor: Environment Diagnostics & Support Bundle

`deacon doctor` reports environment diagnostics (runtime versions, Docker
availability, etc.). The existing `doctor/*` canaries focus on host-requirement
evaluation; this one covers the plain diagnostics output, the `--json`
contract, and `--bundle`.

## Scenarios exercised by `exec.sh`

1. **Text** — `deacon doctor` emits human-readable diagnostics.
2. **JSON** — `deacon doctor --json` emits a single parseable JSON document on
   stdout.
3. **Bundle** — `deacon doctor --bundle <path>` writes a support-bundle
   artifact.

`exec.sh` removes the bundle on exit. No `devcontainer.json` is needed.

## Output streams

Per the output contract, `--json` writes the JSON document to stdout and all
logs to stderr.
