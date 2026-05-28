# Up: `initializeCommand` + Workspace-Trust Gate

The spec's `initializeCommand` runs **on the developer's host**, before any
container is created. Anything host-side that originates in a workspace
file is a supply-chain concern: a hostile `devcontainer.json` checked into
a repository could otherwise execute on `git clone && deacon up`.

Deacon gates every host-side exec site (currently `initializeCommand` and
dotfiles installation) behind an explicit workspace-trust decision. This
gate is **deacon-specific**; the upstream spec doesn't mandate it. See
`SECURITY.md` for the threat model.

## Resolution order (from `crates/core/src/trust.rs`)

1. `--trust-workspace` or `--trust-workspace-persist` → `AlwaysAllow`.
2. `DEACON_NO_PROMPT=1` → `Deny` (CI fail-closed).
3. Default → consult `{user_data_folder}/trusted_workspaces.json`; pass
   only if the canonicalized workspace path is in the store.

On deny, deacon returns `DeaconError::WorkspaceUntrusted` with the
workspace path, the reason, and opt-in instructions naming the flags.

## Files

- `.devcontainer/devcontainer.json` — defines `initializeCommand` (writes
  `.initialize-marker` in the workspace) plus `postCreateCommand` (a
  container-side hook for contrast).

## Scenarios exercised by `exec.sh`

The script uses an isolated `--user-data-folder` so the persistent trust
store is sandboxed and the example never touches the developer's real
allowlist.

1. **Default deny.** `deacon up` against a fresh untrusted workspace.
   Expected: non-zero exit, `WorkspaceUntrusted` in stderr, marker file
   NOT written.
2. **One-shot allow.** `deacon up --trust-workspace`. Marker appears.
   The trust store is unchanged afterwards.
3. **Persistent allow.** Fresh workspace, `deacon up
   --trust-workspace-persist`. Marker appears AND
   `trusted_workspaces.json` now contains the workspace's canonical path.
4. **Persisted re-run.** Subsequent `deacon up` (no flag) against the
   same workspace passes from the store.
5. **CI fail-closed.** `DEACON_NO_PROMPT=1 deacon up` against an
   untrusted workspace exits non-zero without prompting.

## Manual usage

```sh
deacon up --workspace-folder .                  # default: refused
deacon up --workspace-folder . --trust-workspace
deacon up --workspace-folder . --trust-workspace-persist
DEACON_NO_PROMPT=1 deacon up --workspace-folder .   # CI: fail closed
```

## Spec references

- `initializeCommand` and lifecycle phases:
  <https://containers.dev/implementors/spec/#lifecycle-scripts>
- Devcontainer reference (host vs container scripts):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
