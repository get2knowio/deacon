# Parity corpus — RETIRED, kept for its remaining pieces

The in-repo Tier-1 and error corpora, and the Rust runners that drove them, were
**deleted** in 023-migrate-parity-to-conformance (US7) once the equivalence ledger proved
their replacements lose nothing. These five sources were deleted, along with the 24
Tier-1 and 9 error case directories they compared:

- deleted: `crates/deacon/tests/parity_corpus_tier1.rs`
- deleted: `crates/deacon/tests/parity_corpus_merged.rs`
- deleted: `crates/deacon/tests/parity_corpus_errors.rs`
- deleted: `crates/deacon/tests/parity_read_configuration.rs`
- deleted: `crates/deacon/tests/corpus_runner/mod.rs`

Their coverage now lives as declarative cases in `conformance/registry/cases.json`,
driven by the single `parity_conformance_runner`, with fixtures under
`conformance/fixtures/`.

Re-verification of that migration uses **git history**, not a retained copy: the frozen
`conformance/migration/baseline.json` records what each removed unit asserted, and
`conformance/migration/mapping.json` records where each one went.

## What remains here, and why

| Path | Why it survives |
|---|---|
| `oracle.json` | the pinned `@devcontainers/cli` version every live comparison verifies against |
| `registry.json` | the surviving live-binary enumeration (`parity_build`, `parity_exec`, `parity_up_exec`, `parity_observable_state`, `parity_state_diff`, `parity_conformance_runner`) plus the internal-consistency binaries. `corpora` is now empty — the corpora retired with the binaries that drove them |
| `fetch_realworld_corpus.py` | a fetch utility, never a comparison runner; its 33 pinned entries are recorded as `external-corpus-entry` baseline units and covered by `res-realworld-corpus-not-vendored` (research D8) |
| `errors/README.md` | prose describing the error-decision contract, which the declarative `case-errors-decl-*` cases now implement |
| `REPORT.md` | the historical findings log |

## Running the surviving parity suite

```bash
make test-parity              # cargo nextest run --profile parity, then the aggregator
make test-parity-equivalence  # the equivalent-or-stricter ledger that gates a deletion
```

Both need the **pinned** oracle on `PATH`:

```bash
npm install -g @devcontainers/cli@$(jq -r .version fixtures/parity-corpus/oracle.json)
```

Every surviving runner still FAILS LOUDLY — never silently skips — if the oracle is
missing or the wrong version, if a fixture is absent, if a CLI crashes, or if output
cannot be normalized. Raw output is preserved under `target/parity/raw/` and each binary
writes a report fragment under `target/parity/report/`.

The single normalization/equivalence definition lives in
`crates/parity-harness/src/normalize.rs`; the waiver record schema + loader in
`crates/conformance/src/{model,load}.rs`; the parity registry model and the corpus
discovery rule in `crates/conformance/src/parity_corpus.rs`, re-exported by
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
