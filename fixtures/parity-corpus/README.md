# Parity corpus — differential testing against the reference CLI

A curated set of realistic, VS Code-style `devcontainer.json` configs used to
harden deacon against real-world inputs by diffing it against the pinned upstream
reference CLI (`@devcontainers/cli`, the oracle). The pinned version lives in
[`oracle.json`](./oracle.json); the claimed coverage (live binaries + corpora +
minimum case counts) lives in [`registry.json`](./registry.json).

## Layout

Each `<name>/.devcontainer/devcontainer.json` is a realistic config shape
(image + features, Dockerfile build, compose, feature ordering, jsonc with
comments, array/object lifecycle, extends chains, `${...}` substitution, etc.).
Supporting files (Dockerfile, docker-compose.yml, package.json, base.json) live
alongside as needed. `errors/` holds invalid / edge-case inputs (see
[`errors/README.md`](./errors/README.md)); `waivers/` holds observable-state
waiver records (see [`waivers/README.md`](./waivers/README.md)).

## Running the parity suite

The three Python drivers (`run_tier1.py`, `run_tier1_merged.py`,
`run_tier1_errors.py`) were **ported to Rust nextest binaries** and deleted
(018-harden-parity-harness). There is one sanctioned entry point:

```bash
make test-parity
```

which runs `cargo nextest run --profile parity` (the profile selects exactly the
live parity binaries — nothing else runs them) and then the aggregator. The
runners require the **pinned** oracle on `PATH`:

```bash
npm install -g @devcontainers/cli@$(jq -r .version fixtures/parity-corpus/oracle.json)
```

Every runner FAILS LOUDLY (never silently skips) if the oracle is missing or the
wrong version, if a fixture is missing, if a CLI crashes, if output cannot be
normalized, or if a discovered corpus is below its `registry.json` minimum. Both
CLIs' raw output is preserved under `target/parity/raw/` and each binary writes a
report fragment under `target/parity/report/`.

The Rust runners that drive this corpus:

- `crates/deacon/tests/parity_corpus_tier1.rs` — `read-configuration`
  differential (replaces `run_tier1.py`). Runs both CLIs over every case,
  normalizes via `parity_harness::normalize::config` (unwrap the reference's
  `{configuration}` wrapper, prune nulls/empties, drop `configFilePath`, sanitize
  dynamic ids), and ranks divergences: ref-only (deacon drops data — highest
  signal), value mismatch, deacon-only (usually default noise).
- `crates/deacon/tests/parity_corpus_merged.rs` — the same over
  `--include-merged-configuration`, comparing the normalized `mergedConfiguration`
  block (replaces `run_tier1_merged.py`).
- `crates/deacon/tests/parity_corpus_errors.rs` — the error-decision differential
  over `errors/` (replaces `run_tier1_errors.py`); see `errors/README.md`.

The single normalization/equivalence definition lives in
`crates/parity-harness/src/normalize.rs`; the waiver schema/loader in
`crates/parity-harness/src/waiver.rs`; the registry loader in
`crates/parity-harness/src/registry.rs`.

## Tier 3 — pinned real-world corpus fetch

```bash
python3 fixtures/parity-corpus/fetch_realworld_corpus.py --clean --dest /tmp/realworld-corpus
```

`fetch_realworld_corpus.py` (a fetch utility, NOT a comparison runner — it makes
no pass/fail claim) downloads a pinned set of public workspace snapshots into
`/tmp/realworld-corpus` without vendoring third-party content into this
repository. The current manifest mixes:

- `devcontainers/images` workspace subtrees
- two compose-based `devcontainers/templates` workspace subtrees
- `microsoft/vscode-remote-try-*` sample repos
- a couple of small real OSS repos with checked-in devcontainers

The fetched corpus includes a `_manifest.json` recording the exact repos and
commit SHAs used for the run. It is for manual exploration; the pinned, in-repo
corpus above is what the nextest runners drive.

## Tier 2 — up/build (Docker)

Covered by the container-scenario parity binaries (`parity_up_exec`,
`parity_build`, `parity_observable_state`, `parity_state_diff`) under the same
`--profile parity`. They copy configs to a TempDir **outside** the repo (in-repo
`up` chowns the workspace and mounts the git root) and bring the container up with
`--trust-workspace` (deacon-specific host-trust gate; the reference has no such
gate).

See `REPORT.md` for findings.
