# Features: Lockfile Lifecycle (`--frozen-lockfile`)

When a config declares OCI Features, deacon resolves them to content digests
and records them in `devcontainer-lock.json` so later builds are reproducible.
This canary exercises generation and the `--frozen-lockfile` gate.

## Files

- `.devcontainer/devcontainer.json` — one OCI feature
  (`ghcr.io/devcontainers/features/git:1`). The lockfile is generated/removed
  by the script.

## Scenarios exercised by `exec.sh`

1. **Generate** — a plain `up` writes `devcontainer-lock.json` with a pinned
   `sha256:` digest.
2. **Frozen passes** — `up --frozen-lockfile` succeeds when the lockfile
   matches the resolved Features.
3. **Frozen fails closed** — after tampering the pinned digest,
   `up --frozen-lockfile` exits non-zero rather than silently re-resolving.

## Notes

Depends on Docker **and** the public `ghcr.io` registry. An unreachable
registry is environmental, not a deacon defect.
