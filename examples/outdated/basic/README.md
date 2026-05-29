# Outdated: Feature Version Drift Report

`deacon outdated` resolves the Features in a config against their registry and
reports `current | wanted | latest`, so you can see which Features have newer
versions available. It needs network access to the feature registry
(`ghcr.io`) but does **not** start a container.

## Files

- `.devcontainer/devcontainer.json` — pins
  `ghcr.io/devcontainers/features/git:1.0.0` (deliberately old) so it shows as
  outdated.

## Scenarios exercised by `exec.sh`

1. **Report** — `outdated --output json` emits a non-empty JSON object that
   includes the `git` feature.
2. **CI gate** — `outdated --fail-on-outdated` exits non-zero because the
   pinned version is behind `latest`.

## Output streams

`--output json` writes the JSON report to stdout; all logs go to stderr.

## Notes

This canary depends on the public `ghcr.io` registry. If the registry is
unreachable it cannot pass (environmental, not a deacon defect).
