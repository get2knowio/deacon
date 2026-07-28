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

## Tier 3 — pinned real-world corpus — MOVED

`fetch_realworld_corpus.py` was **deleted** in 025-exploratory-parity-discovery (US7,
T109). Its 33 pinned entries now live in `conformance/discovery/corpus.json`, a
Rust-owned strict-JSON manifest, and the fetch lives in
`crates/parity-harness/src/discovery/corpus_fetch.rs`.

The script was not retired for tidiness. Two concrete reasons:

1. **The immutable-reference check has to be hermetic.** FR-050 — reject any branch, tag,
   `HEAD`, or `latest` — is a property of the *manifest*, not of a fetch: nothing needs to
   be retrieved to know that `main` names different content tomorrow. In a Python tuple
   nothing checked it; as strict JSON, violation class **D4** rejects it on every pull
   request with no network at all (research D8).
2. **Two statements of one manifest drift.** The frozen `realworld::<name>` baseline units
   are derived from the entry names, and the corpus tier compares against the entry pins.
   With two copies, one of those two would eventually be reading the other's stale twin.

Its documented workflow had also stopped existing: the docstring told you to copy a
fetched snapshot into the corpus root and run `parity_corpus_tier1` — deleted by 023, as
was `parity_corpus_merged`, along with the corpus root itself. A
script whose instructions name removed binaries is not an exploratory aid.

What replaced it is strictly more, not less:

- the entries are validated (**D4**) rather than merely written down;
- each entry carries a **content digest**, recorded at first materialization and verified
  on every later fetch (FR-051) — the Python fetcher verified nothing;
- an unreachable entry is reported as unreachable, never as "ran and found nothing"
  (FR-052);
- the fetch uses a blob-filtered partial clone plus a sparse checkout instead of the
  GitHub contents API, so it needs no `gh`, no token, and no rate-limit budget.

Corpus content is still **never vendored** (FR-053): the manifest records provenance, not
bytes. To run the canary:

```bash
cargo run -p parity-harness --bin discovery-campaign -- \
  --seed 0x… --tier corpus --budget-seconds 1800 --lane invoked
```

It also runs weekly in `.github/workflows/discovery.yml`, on a schedule of its own —
an ecological canary that runs only when someone remembers to invoke it cannot warn
anyone (FR-056, research D10).

## Tier 2 — up/build (Docker)

Covered by the container-scenario parity binaries (`parity_up_exec`,
`parity_build`, `parity_observable_state`, `parity_state_diff`) under the same
`--profile parity`. They copy configs to a TempDir **outside** the repo (in-repo
`up` chowns the workspace and mounts the git root) and bring the container up with
`--trust-workspace` (deacon-specific host-trust gate; the reference has no such
gate).

See `REPORT.md` for findings.
