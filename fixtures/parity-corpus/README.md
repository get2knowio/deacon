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

## Tier 1c — error-path differential (fast, Docker-free)

```bash
python3 fixtures/parity-corpus/run_tier1_errors.py [deacon_bin] [corpus_dir]
```

Where Tier 1 diffs *successful* output, Tier 1c diffs the **accept/reject
decision** over invalid / edge-case configs under `errors/`: do both CLIs agree
on whether the input is an error? Exits non-zero on any divergence from each
fixture's encoded expectation (CI-gateable). It surfaced that deacon validates
eagerly/strictly at `read-configuration` while the reference parses leniently and
defers `extends` resolution — see `errors/README.md` for the full matrix and why
those divergences are characterized rather than treated as bugs.

## Tier 3 — pinned real-world corpus fetch

```bash
python3 fixtures/parity-corpus/fetch_realworld_corpus.py --clean --dest /tmp/realworld-corpus
python3 fixtures/parity-corpus/run_tier1.py target/debug/deacon /tmp/realworld-corpus
python3 fixtures/parity-corpus/run_tier1_merged.py target/debug/deacon /tmp/realworld-corpus
```

`fetch_realworld_corpus.py` downloads a pinned set of public workspace snapshots
into `/tmp/realworld-corpus` without vendoring third-party content into this
repository. The current manifest mixes:

- `devcontainers/images` workspace subtrees
- two compose-based `devcontainers/templates` workspace subtrees
- `microsoft/vscode-remote-try-*` sample repos
- a couple of small real OSS repos with checked-in devcontainers

The fetched corpus includes a `_manifest.json` file recording the exact repos and
commit SHAs used for the run.

## Tier 2 — up/build (Docker)

Copy a config to a TempDir **outside** the repo (in-repo `up` chowns the
workspace and mounts the git root), then:

```bash
deacon up --workspace-folder <tmp> --remove-existing-container --trust-workspace
```

`--trust-workspace` satisfies deacon's host-side trust gate (deacon-specific;
the reference has no such gate). Assert the container starts and lifecycle runs.

See `REPORT.md` for findings.
