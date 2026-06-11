# Parity corpus — differential testing against the reference CLI

A curated set of realistic, VS Code-style `devcontainer.json` configs used to
harden deacon against real-world inputs by diffing it against the upstream
reference CLI (`@devcontainers/cli`, the oracle).

## Layout

Each `<name>/.devcontainer/devcontainer.json` is a realistic config shape
(image + features, Dockerfile build, compose, feature ordering, jsonc with
comments, array/object lifecycle, extends chains, `${...}` substitution, etc.).
Supporting files (Dockerfile, docker-compose.yml, package.json, base.json) live
alongside as needed.

## Tier 1 — read-configuration differential (fast, Docker-free)

```bash
python3 fixtures/parity-corpus/run_tier1.py [deacon_bin] [corpus_dir]
```

Runs `read-configuration` through both CLIs, normalizes (unwrap the reference's
`{configuration}` wrapper, strip nulls/empties/defaults, drop `configFilePath`),
deep-diffs, and prints a ranked report:

- `❗ ref-only` — the reference populates a field deacon drops (highest signal).
- `⚡ mismatch` — both populate a field with different values.
- `· deacon-only` — usually default/serialization noise.

Requires the reference CLI on `PATH` (`npm i -g @devcontainers/cli`).

## Tier 2 — up/build (Docker)

Copy a config to a TempDir **outside** the repo (in-repo `up` chowns the
workspace and mounts the git root), then:

```bash
deacon up --workspace-folder <tmp> --remove-existing-container --trust-workspace
```

`--trust-workspace` satisfies deacon's host-side trust gate (deacon-specific;
the reference has no such gate). Assert the container starts and lifecycle runs.

See `REPORT.md` for findings.
