# Upgrade: Regenerate the Feature Lockfile

`deacon upgrade` re-resolves the Features declared in a config and (re)writes
`devcontainer-lock.json`, pinning each Feature to a content digest. It needs
network access to the feature registry (`ghcr.io`) but does not start a
container. `--dry-run` prints the lockfile JSON to stdout instead of writing it.

## Files

- `.devcontainer/devcontainer-lock.json` is generated/removed by the script.
- `.devcontainer/devcontainer.json` — one OCI feature
  (`ghcr.io/devcontainers/features/git:1`).

## Scenarios exercised by `exec.sh`

1. **`--dry-run`** prints a resolved, digest-pinned (`sha256:`) lockfile to
   stdout as valid JSON and does **not** write to disk.
2. **`upgrade`** (no flag) writes `devcontainer-lock.json` with a pinned digest.

## Notes

Depends on the public `ghcr.io` registry; unreachable registry → environmental
failure, not a deacon defect.
