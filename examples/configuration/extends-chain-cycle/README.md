# Configuration: Extends-Chain Cycle Detection

`extends` lets a `devcontainer.json` inherit from another. The chain is
single-parent — each file `extends` at most one other — so the resolver
walks a linked list. The spec requires the resolver to **detect and
reject cycles** rather than infinite-loop.

This example provides two cycle shapes:

- `alpha.json` ↔ `bravo.json` — two-node cycle (alpha extends bravo,
  bravo extends alpha).
- `self.json` — degenerate one-node cycle (a file that extends
  itself).

`exec.sh` invokes `read-configuration` against each and asserts the
process exits non-zero with a "cycle" or "extends" message in stderr.

## Files

- `alpha.json`, `bravo.json` — two-file cycle.
- `self.json` — one-file cycle (extends itself).

## Scenarios exercised by `exec.sh`

1. **Two-file cycle rejected.** `deacon read-configuration --config
   ./alpha.json` exits non-zero; stderr mentions the cycle / extends.
2. **One-file cycle rejected.** `--config ./self.json` exits non-zero.
3. **Diagnostic clarity.** Stderr names at least one of the cycle's
   participants by path so the user can find what to fix.

## Manual usage

```sh
deacon read-configuration --config ./alpha.json   # expect non-zero exit
deacon read-configuration --config ./self.json    # expect non-zero exit
```

## Known deacon issues this example surfaces

- [#65](https://github.com/get2knowio/deacon/issues/65) — deacon over-validates
  `--config` filenames. Until that lands, the literal names `alpha.json` /
  `bravo.json` / `self.json` are rejected with "Invalid --config filename".
  The upstream `@devcontainers/cli` accepts any filename.
- [#66](https://github.com/get2knowio/deacon/issues/66) — `read-configuration`
  demands `--workspace-folder` even when `--config` is provided. Until that
  lands, an extra `--workspace-folder .` is needed.

## Spec references

- `extends`:
  <https://containers.dev/implementors/json_reference/>
- Devcontainer reference (configuration resolution):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
