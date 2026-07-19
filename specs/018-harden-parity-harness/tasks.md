# Tasks: Harden the Parity Test Harness

**Input**: Design documents from `/specs/018-harden-parity-harness/`
**Prerequisites**: plan.md, spec.md, research.md (D1–D12), data-model.md, contracts/, quickstart.md

**Tests**: INCLUDED — the spec mandates acceptance tests (FR-021..FR-024); harness modules get unit tests per constitution VII.

**Organization**: Grouped by user story. Foundational phase builds the `parity-harness` crate core that every story consumes (shared-entity rule); each story phase is then an independently verifiable increment.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US5 from spec.md

## Path Conventions

Workspace: `crates/parity-harness/` (new, publish = false), `crates/deacon/tests/`, `fixtures/parity-corpus/`, `.config/nextest.toml`, `.github/workflows/`, `Makefile`.

---

## Phase 1: Setup

**Purpose**: Crate scaffold + authoritative pin, so all subsequent work has a home.

- [X] T001 Create `crates/parity-harness/` crate: `Cargo.toml` (name `parity-harness`, `publish = false`, edition 2024, deps: `serde`, `serde_json`, `thiserror`, `tokio` [process, time, fs, io-util, rt], `tracing`, `chrono` [workspace-existing, RFC3339 report timestamps], `toml` [NEW dev-scope dep — nextest.toml parsing for the registry check]) and empty module skeleton `src/lib.rs` declaring `oracle`, `prereq`, `exec`, `normalize`, `waiver`, `registry`, `report` modules; register as workspace member in root `Cargo.toml` and as a dev-dependency of `crates/deacon/Cargo.toml`
- [X] T002 [P] Create the single authoritative oracle pin `fixtures/parity-corpus/oracle.json` with `{ "package": "@devcontainers/cli", "version": "0.87.0" }` (data-model §1, research D1)
- [X] T003 Verify green scaffold: `cargo fmt --all && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo build -p parity-harness`

**Checkpoint**: workspace builds with the empty crate; pin file exists.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The harness core every story consumes: error taxonomy, verified-oracle acquisition, prerequisite checks, bounded execution with raw capture, fragment writing, and the single normalization module. No user story can start before this phase completes.

- [X] T004 Implement `HarnessError` taxonomy in `crates/parity-harness/src/lib.rs` (`thiserror`): `OracleMissing{hint}`, `OracleVersionMismatch{found, required, path}`, `OracleUnverifiable{path, cause}`, `OracleFailure{case, status, stderr_path}`, `OracleTimeout{case, bound, partial_paths}`, `MalformedOutput{case, cause}`, `DockerMissing`, `FixtureMissing{path}`, `Normalization{case, cause}`, `WaiverStale{id}`, `WaiverInvalid{path, cause}`, `Report{cause}`, `CorpusTooSmall{corpus, found, min}` — Display strings name cause + remedy (data-model §9, FR-005)
- [X] T005 Implement `crates/parity-harness/src/oracle.rs`: `OraclePin` loaded via `include_str!("../../../fixtures/parity-corpus/oracle.json")` + serde (`deny_unknown_fields`); `Oracle::acquire()` resolving `DEACON_PARITY_DEVCONTAINER` override else `PATH`, running `--version` under a 2-minute bound, comparing EXACTLY to the pin, caching `VerifiedOracle{path, source, version}` in a process-wide `OnceLock<Result<…>>`; version-query timeout/garbage → `OracleUnverifiable` (research D3, FR-001..FR-003, edge cases "two oracles"/"garbage version")
- [X] T006 [P] Implement `crates/parity-harness/src/prereq.rs`: `require_docker()` (honoring `DEACON_PARITY_DOCKER` override as the fault-injection seam, else `docker` on PATH, probing `docker version`) and `require_fixture(path)` — both return cause-specific `HarnessError`s, never booleans (research D3/D10, FR-005)
- [X] T007 Implement `crates/parity-harness/src/exec.rs`: `ExecKind::{Config, Lifecycle}` with bounds 2 min / 15 min (bounds injectable for tests; bounds are PER CLI INVOCATION, not per test), `exec_oracle`/`exec_deacon` via `tokio::process::Command` + `tokio::time::timeout` — `exec_deacon` takes the deacon binary path as an explicit argument (test binaries pass `env!("CARGO_BIN_EXE_deacon")`; parity-harness cannot use that macro itself) — ALWAYS capturing raw stdout/stderr to `<report_dir>/raw/<binary>/<case>/{deacon,oracle}.{stdout,stderr}` where `<report_dir>` defaults to `<workspace_root>/target/parity/` ANCHORED TO THE WORKSPACE ROOT (derive from `CARGO_MANIFEST_DIR`; cargo-test CWD is the package dir, a bare relative `target/` would scatter artifacts), overridable via `DEACON_PARITY_REPORT_DIR`; atomic temp+rename writes; timeout → `OracleTimeout` with partial output preserved; nonzero-exit-where-success-expected → `OracleFailure` (research D3/D8, FR-007, FR-020)
- [X] T008 Implement `crates/parity-harness/src/report.rs`: `ReportFragment`/`CaseResult` types per contracts/report-schema.md (`mode: "live"` mandatory, `cause` enum, `waivers_applied`, `raw` paths), fragment writer to `target/parity/report/<binary>.json` (atomic); write failure returns `HarnessError::Report` which the caller MUST propagate as test failure (data-model §5, FR-016, FR-018)
- [X] T009 Implement `crates/parity-harness/src/normalize.rs` — THE single equivalence definition (research D7, FR-019): `config()` porting the Python `prune` semantics (unwrap `{configuration}`, drop `configFilePath`, prune nulls/empty containers, sanitize hex-12/`${devcontainerId}` → `<ID>`), `merged_config()` (config rules on the `mergedConfiguration` block), `container_state()` (move noise-env subtraction, intentional-label prefixes, project-prefix stripping, user normalization from `crates/deacon/tests/parity_utils.rs` L488–981); all return `Result<_, HarnessError>` — no fallback paths; plus `diff` producing ranked ref-only / deacon-only / value-mismatch summaries
- [X] T010 [P] Unit tests for oracle/prereq/exec/report in `crates/parity-harness/src/` module `#[cfg(test)]` blocks: pin parse, exact-version match/mismatch, override-beats-PATH resolution, timeout with injectable bound, atomic fragment write, malformed-pin rejection (constitution VII; hermetic, no network)
- [X] T011 [P] Unit tests for `normalize` in `crates/parity-harness/src/normalize.rs`: prune semantics, dynamic-id sanitization, `container_state` noise subtraction, normalization-failure on invalid input returns `Normalization` error (feeds SC-005 groundwork)
- [X] T012 Gate: `cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo nextest run -p parity-harness`

**Checkpoint**: harness crate complete and unit-tested; nothing consumes it yet — all existing tests still green.

---

## Phase 3: User Story 1 — A passing parity result proves the comparison ran (Priority: P1) 🎯 MVP

**Goal**: Every live parity binary fails loudly (never skips) on missing/wrong oracle, missing Docker, missing fixture, oracle crash, malformed output, or normalization failure; verified oracle recorded per run. Selection moves to a dedicated nextest profile so other lanes are truthful by non-selection.

**Independent Test**: `DEACON_PARITY_DEVCONTAINER=/nonexistent cargo nextest run --profile parity` → every selected test FAILS naming the oracle; with a stub printing `0.86.0` → fails naming found-vs-required; with the real 0.87.0 oracle (+ Docker) → live comparisons run and fragments record the verified version. `make test-nextest` (no oracle) stays green with parity visibly not selected.

### Implementation for User Story 1

- [X] T013 [US1] Restructure `.config/nextest.toml`: add `[profile.parity]` whose `default-filter` selects exactly the live parity binaries (`parity_read_configuration`, `parity_exec`, `parity_build`, `parity_up_exec`, `parity_observable_state`, `parity_state_diff` — corpus binaries join in US2); REMOVE those binaries from selection in `default`, `full`, `docker`, `ci`, and `mvp-integration` default-filters (`dev-fast` already excludes); keep `parity`/`parity-cli` group assignments + 15 m slow-timeouts inside the new profile (research D2, contracts/execution-contract.md, FR-014). NOTE: land in the same commit as T014–T019 so no lane ever runs fail-fast binaries without prerequisites
- [X] T014 [US1] Migrate `crates/deacon/tests/parity_read_configuration.rs`: replace `upstream_available()` early-returns with `Oracle::acquire()` + `require_fixture` fail-fast, bounded `ExecKind::Config` execution, `normalize::config` comparison, fragment + raw-output writing via harness (FR-002, FR-004..FR-006)
- [X] T015 [P] [US1] Migrate `crates/deacon/tests/parity_exec.rs`: same pattern + `require_docker()` before container scenarios; `ExecKind::Lifecycle` bounds; fragment writing
- [X] T016 [P] [US1] Migrate `crates/deacon/tests/parity_build.rs`: same pattern (docker-required, Lifecycle bounds, fragments)
- [X] T017 [P] [US1] Migrate `crates/deacon/tests/parity_up_exec.rs`: same pattern and ADD the missing Docker prerequisite check (today it gates only on the oracle)
- [X] T018 [P] [US1] Migrate `crates/deacon/tests/parity_observable_state.rs`: delete its local `gated()` (L23) and all early returns; use harness prereqs/exec/`normalize::container_state`/fragments; leave `KNOWN_*` const deletion to US2 (T024)
- [X] T019 [P] [US1] Migrate `crates/deacon/tests/parity_state_diff.rs`: same pattern via shared harness helpers
- [X] T020 [US1] Delete `crates/deacon/tests/parity_utils.rs` and purge every remaining reference to `DEACON_PARITY`, `DEACON_PARITY_UPSTREAM_READ_CONFIGURATION`, `gated(`, `upstream_available(` from the repo (`sg`/grep sweep); the read-configuration arg template becomes a harness-internal default (research D3/D12, FR-006)
- [X] T021 [US1] Gate: `cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && make test-nextest-fast` green WITHOUT any oracle installed; then `cargo nextest run --profile parity` observed to FAIL-fast with `OracleMissing` messaging (the US1 independent test, negative half)

**Checkpoint**: US1 delivers the MVP trust property for all six existing live binaries.

---

## Phase 4: User Story 2 — One execution contract with explicit pass/fail for every runner (Priority: P2)

**Goal**: Corpus runners become Rust nextest binaries with nonzero-on-divergence semantics; one waiver schema/loader governs all characterized divergences; the registry enumerates coverage; oracle-free binaries stop claiming parity.

**Independent Test**: temporarily inject a differing key into a corpus fixture → `cargo nextest run --profile parity -E 'binary(=parity_corpus_tier1)'` fails naming the case; add a waiver record → passes with the waiver id in the fragment; remove the injected difference (waiver now stale) → fails naming the record. `python3 fixtures/parity-corpus/run_tier1.py` no longer exists.

### Implementation for User Story 2

- [X] T022 [US2] Implement `crates/parity-harness/src/waiver.rs`: `Waiver{id, scope, expect, rationale, added}` with tagged `Scope`/`Expect` unions per contracts/registry-waiver-schema.md (`deny_unknown_fields`, trailing-`*` field prefixes), loader over `errors/*/expect.json` + `waivers/*.json`, uniqueness validation, and staleness evaluation (`active` iff matches an existing case AND the expected difference is observed) returning `WaiverStale`/`WaiverInvalid` errors (research D6, FR-010..FR-012); unit tests in-module
- [X] T023 [P] [US2] Upgrade all 9 `fixtures/parity-corpus/errors/*/expect.json` records to the full waiver schema: add `id` (`errors/<case>`), `scope`, `rationale` (from the Tier 1c characterization / CLAUDE.md "Verified Non-Bugs"), `added`; keep existing `expect` kinds and `signal`/`config` fields (data-model §4 migration note)
- [X] T024 [US2] Delete `KNOWN_INTENTIONAL_DIVERGENCES`/`KNOWN_GAPS`/`KnownGap`/`classify_divergence`-era const machinery from the observable-state path (now in harness after T018) and wire `parity_observable_state.rs` + `parity_state_diff.rs` divergence classification through `waiver::load` with `fixtures/parity-corpus/waivers/` (create the dir with a README stub; both const lists are empty today so no records to migrate) (research D6)
- [X] T025 [US2] Implement `crates/parity-harness/src/registry.rs`: `ParityRegistry{live_binaries, internal_consistency_binaries, corpora}` loader + validation helpers (file↔registry bidirectional match, profile-filter cross-check, corpus min-count check) per contracts/registry-waiver-schema.md (research D5, FR-022, FR-024); unit tests in-module
- [X] T026 [US2] Create `fixtures/parity-corpus/registry.json` enumerating the 9 live binaries (6 scenario + 3 corpus), the 2 internal-consistency binaries, and corpora `tier1` (path `fixtures/parity-corpus`, `min_cases: 20`) and `errors` (path `fixtures/parity-corpus/errors`, `min_cases: 9`)
- [X] T027 [US2] Port `run_tier1.py` → `crates/deacon/tests/parity_corpus_tier1.rs`: one `#[test]` — discover cases (`_manifest.json` else dir-scan restricted to IMMEDIATE subdirectories of the corpus root containing `.devcontainer/`, explicitly excluding `errors/`, `waivers/`, `__pycache__/`, and dot-dirs — `errors/*` cases also contain `.devcontainer/` and belong only to the errors runner), fail `CorpusTooSmall` below registry minimum, run both CLIs via harness (`ExecKind::Config`), compare via `normalize::config` + ranked diff, apply corpus-case waivers, write fragment, fail listing every offending case on any unwaived ref-only/deacon-only/value/process/normalization failure (research D4, FR-009, FR-024)
- [X] T028 [P] [US2] Port `run_tier1_merged.py` → `crates/deacon/tests/parity_corpus_merged.rs`: same skeleton with `--include-merged-configuration` both sides and `normalize::merged_config`
- [X] T029 [P] [US2] Port `run_tier1_errors.py` → `crates/deacon/tests/parity_corpus_errors.rs`: accept/reject decision matrix (`both-reject` / `both-accept` / `deacon-stricter`) driven by the schema-validated waiver records; value-compare on both-accept; stale/missing expectation → failure
- [X] T030 [US2] Delete `fixtures/parity-corpus/run_tier1.py`, `run_tier1_merged.py`, `run_tier1_errors.py`, `__pycache__/`; update `fixtures/parity-corpus/README.md` and `errors/README.md` to document the Rust runners, registry, waiver schema, and `make test-parity` invocation (keep `fetch_realworld_corpus.py` — utility, not a runner)
- [X] T031 [P] [US2] Rename `crates/deacon/tests/parity_remote_env_flags.rs` → `consistency_remote_env_flags.rs` and `parity_env_probe_flag.rs` → `consistency_env_probe_flag.rs` (git mv; update header comments to "internal-consistency, no oracle") (research D9, FR-013)
- [X] T032 [US2] Update `.config/nextest.toml` for US2 binaries across ALL profiles (3-spot rule per binary): add `parity_corpus_tier1|merged|errors` to `[profile.parity]` selection with `parity-cli` group and a 15 m slow-timeout each (the 2 min bound is per CLI invocation; one corpus `#[test]` runs ~23 cases × 2 CLIs, so a 2 m per-test timeout would kill healthy runs); assign renamed `consistency_*` binaries to their prior non-parity groups in every profile and ensure they remain selected in fast/default lanes and are NOT in `[profile.parity]`
- [X] T033 [US2] Gate: fmt + clippy + `make test-nextest-fast` green (consistency binaries still run; corpus binaries excluded without oracle); run the US2 independent test end-to-end if the pinned oracle is installed locally (inject → fail; waive → pass-waived; stale → fail), else record as certification-lane verification for T041

**Checkpoint**: the entire parity surface is nextest-gateable Rust with explicit semantics; Python runners gone.

---

## Phase 5: User Story 3 — CI status that cannot claim unearned parity coverage (Priority: P3)

**Goal**: Aggregated run report proves execution completeness against the registry; certification lane provisions the pinned oracle and gates; `make test-parity` is a thin alias; lane naming distinguishes live certification.

**Independent Test**: delete one fragment from `target/parity/report/` → `cargo run -p parity-harness --bin parity-report` exits nonzero listing `missing_fragments`; full local run with oracle produces `parity-report.json` with verified version 0.87.0; the new workflow fails if `npm i -g` is pointed at any other version.

### Implementation for User Story 3

- [X] T034 [US3] Implement aggregator `crates/parity-harness/src/bin/parity-report.rs`: read registry + all fragments from `DEACON_PARITY_REPORT_DIR` (default `target/parity/`), enforce the six gate conditions of contracts/report-schema.md (missing fragments, oracle consistency vs pin, zero failures, explained omissions, zero stale waivers, corpus minimums), write `target/parity/parity-report.json` atomically, exit nonzero on any gap with an enumerating message (research D8, FR-016, FR-018, FR-022; SC-003)
- [X] T035 [P] [US3] Unit/integration tests for the aggregator in `crates/parity-harness/tests/aggregator.rs` using fabricated fragment fixtures in a temp report dir: all-green pass, missing-fragment fail, oracle-mismatch-across-fragments fail, stale-waiver fail, unexplained-omission fail, corpus-below-minimum fail, unwritable-report-dir fail (FR-018)
- [X] T036 [US3] Rewrite `Makefile` `test-parity` as the thin alias — `cargo nextest run --profile parity` then `cargo run -p parity-harness --bin parity-report` — deleting the `cargo test` invocation, the `command -v devcontainer` gating, and all `DEACON_PARITY*` env plumbing; keep `test-parity-all` as alias (research D12, contracts/execution-contract.md)
- [X] T037 [US3] Create `.github/workflows/parity.yml` — check name `parity / live-certification`: triggers `schedule` (nightly cron, main), `workflow_dispatch`, `pull_request` with `paths:` [`crates/parity-harness/**`, `crates/deacon/tests/parity_*`, `crates/deacon/tests/consistency_*`, `fixtures/parity-corpus/**`, `.config/nextest.toml`, `Makefile`, `.github/workflows/parity.yml`]; steps: checkout, Node 20, `npm install -g @devcontainers/cli@$(jq -r .version fixtures/parity-corpus/oracle.json)`, echo `devcontainer --version`, install cargo-nextest, build deacon, `cargo nextest run --profile parity`, `cargo run -p parity-harness --bin parity-report`, `actions/upload-artifact` of `target/parity/` (always(), so red runs keep diagnostics); NO `continue-on-error` anywhere (research D11, FR-015, FR-017)
- [X] T038 [US3] Truthfulness sweep of existing lanes: verify `.github/workflows/ci.yml` jobs and the `default`/`full`/`docker`/`ci`/`mvp-integration` profiles neither select live parity binaries nor mention parity coverage in names/step-echoes; adjust any stale wording (e.g. Makefile help text, `make test-nextest-audit` expectations) (FR-014, SC-007)
- [X] T039 [US3] Gate + independent test: fabricate a fragment dir, run aggregator pass/fail cases from T035 via nextest; `make test-parity` against a missing oracle exits nonzero at the nextest step with `OracleMissing` messages (alias has no logic of its own)

**Checkpoint**: reports + aggregation + certification lane complete; CI can no longer claim unearned parity.

---

## Phase 6: User Story 4 — One definition of equivalence, raw outputs preserved (Priority: P4)

**Goal**: Prove (not just implement) single-verdict equivalence and always-diagnosable failures. The shared module landed in Phase 2; this phase verifies consolidation and closes residual duplication.

**Independent Test**: the same differing output pair evaluated through the tier1 runner path and the read-configuration binary path yields the same verdict; a failing case's fragment links raw files that exist and contain the verbatim CLI bytes.

### Implementation for User Story 4

- [X] T040 [P] [US4] Cross-runner equivalence tests in `crates/parity-harness/tests/normalize_consistency.rs`: table of output pairs (equal-after-prune, ref-only key, deacon-only key, value mismatch, dynamic-id-only difference, malformed JSON) asserting `normalize::config` + `diff` produce identical verdicts regardless of caller context, and that `merged_config` agrees with `config` on the shared block (SC-005, FR-019)
- [X] T041 [US4] Residual-duplication audit: `sg`/grep sweep proving no second normalization implementation survives (no `extract_core_config`, no `sanitize_dynamic_values` outside `normalize.rs`, no `prune(` in fixtures scripts — Python already deleted in T030); document the single-module guarantee in `crates/parity-harness/src/normalize.rs` module docs (FR-019)
- [X] T042 [P] [US4] Raw-preservation integration test in `crates/parity-harness/tests/raw_outputs.rs`: drive `exec` + `report` against stub executables producing known bytes on stdout/stderr; assert all four raw files exist per case with verbatim contents, fragment `raw` paths resolve, and a read-only raw dir fails the run with `Report`/`Normalization`-class error rather than passing (FR-018, FR-020, SC-006)

**Checkpoint**: equivalence is provably single-sourced; every comparison is diagnosable from artifacts.

---

## Phase 7: User Story 5 — Acceptance tests that prove the harness cannot lie (Priority: P5)

**Goal**: Permanent, hermetic regression guard: every guaranteed failure mode demonstrably fails; registry completeness enforced structurally on every PR and at certification time.

**Independent Test**: `cargo nextest run -E 'binary(=parity_harness_faults) or binary(=parity_registry_check)'` passes on a machine with NO oracle, NO Docker, NO network.

### Implementation for User Story 5

- [X] T043 [US5] Implement `crates/deacon/tests/parity_harness_faults.rs` fault-injection suite via stub executables + env overrides (`#[cfg(unix)]` on stub-script cases with one-line reasons; each stub written to a per-test temp dir): (a) wrong-version stub `0.86.0` → `OracleVersionMismatch` naming found vs required 0.87.0; (b) nonexistent override path → `OracleMissing` with provisioning hint; (c) failing docker stub via `DEACON_PARITY_DOCKER` → `DockerMissing`; (d) crash stub (exit 1 mid-protocol) → `OracleFailure`; (e) garbage-JSON stub → `MalformedOutput`; (f) version-query hang stub with test-shortened bound → `OracleTimeout` with partial output preserved (research D10, FR-021; SC-001)
- [X] T044 [P] [US5] Extend `parity_harness_faults.rs` with the comparison-pipeline injections: (g) fabricated differing documents → unwaived-divergence failure; (h) + matching waiver fixture → `pass-waived` referencing the record id; (i) difference removed, waiver kept → `WaiverStale`; (j) invalid input to `normalize::config` → `Normalization` failure, assert no raw-comparison fallback occurred (FR-021 injected-difference + normalization-failure legs)
- [X] T045 [US5] Implement `crates/deacon/tests/parity_registry_check.rs` (hermetic, all lanes): registry↔`tests/*.rs` bidirectional match; parse `.config/nextest.toml` via the `toml` crate (declared in T001) and assert `[profile.parity]` covers exactly `live_binaries` and excludes `internal_consistency_binaries` and that no other profile selects live parity binaries; corpus dirs meet `min_cases`; source audit of all `parity_*`/`consistency_*` test files rejecting `#[ignore]`, `gated(`, `upstream_available(`, `DEACON_PARITY` legacy idioms (research D5, FR-013, FR-022..FR-024; SC-003)
- [X] T046 [US5] Add ALL new hermetic test binaries to `.config/nextest.toml` in ALL profiles (3-spot rule; constitution VII): `parity_harness_faults` + `parity_registry_check` (crates/deacon) AND the parity-harness integration-test binaries from T035/T040/T042 (`aggregator`, `normalize_consistency`, `raw_outputs`) — selected in `default`, `dev-fast`, `full`, `ci`, `mvp-integration` (hermetic groups — `fs-heavy` for faults/raw_outputs/aggregator, default group for registry check and normalize_consistency), NONE selected in `[profile.parity]`; verify with `make test-nextest-audit`
- [X] T047 [US5] Gate: `cargo nextest run -E 'binary(=parity_harness_faults) or binary(=parity_registry_check)'` green hermetically; temporarily comment a registry entry → registry check fails (restore); confirm Windows-compilability of the gated suites (`cargo check --tests -p deacon` mental pass: `#[cfg(unix)]` attributes, not `cfg!`, per repo Windows notes)

**Checkpoint**: the harness's trust property is self-guarding on every PR.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T048 [P] Documentation sync: update `CLAUDE.md` (parity section: profile-based selection, retired `DEACON_PARITY`, new crate, registry/waiver model, certification lane), `docs/parity-state-diff.md`, and `docs/ROADMAP_TO_1.0.md` references to the runners/oracle pinning; note the `parity / live-certification` check name
- [X] T049 [P] Add `parity-harness` to release-hygiene surfaces: confirm `cargo fmt`/`clippy --all-features` cover it, coverage config (`cargo-llvm-cov`) doesn't break on the bin target, and `make release-check` passes
- [X] T050 Full local validation: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest` (green, live parity not selected), then quickstart.md end-to-end with the real oracle if available locally (`npm i -g @devcontainers/cli@0.87.0`, `make test-parity`, inspect `target/parity/parity-report.json`); any real divergences surfaced by the widened comparison are triaged per spec Out-of-Scope: fix-or-characterize follow-ups filed, NOT silently waived in this PR
  - fmt/clippy clean. `make test-nextest` (`--profile full`): **3193 passed, 0 failed, 38 skipped** in 657s — skip count includes exactly the 9 registered live parity binaries via `profile.full.default-filter`, confirming truthful non-selection outside `--profile parity`.
  - No `@devcontainers/cli` oracle available in this sandbox, so the live-oracle end-to-end leg (`make test-parity` against a real 0.87.0 install, inspecting `target/parity/parity-report.json` for real divergences) could not be run here; that verification is deferred to the `parity / live-certification` CI lane (T037/T051) and to a future local run with the oracle installed. The fail-fast half was reconfirmed instead: `make test-parity` with no oracle fails at the nextest step naming `OracleMissing`, never silently passing.
- [X] T051 Verify the certification lane on the feature branch: push, confirm `parity / live-certification` triggers via its `paths:` filter, runs green (or red for characterized reasons with artifacts uploaded), and `ci.yml` lanes remain green — the US3/SC-007 truthful-status proof in situ
  - Pushed `018-harden-parity-harness`, opened PR #306. `parity / live-certification` triggered correctly via its `paths:` filter (first-ever real run: provisioned `@devcontainers/cli@0.87.0`, ran `--profile parity` against real Docker). All `ci.yml` jobs stayed green (parity truthfully excluded from their selection, confirming FR-014/SC-007) — the exact truthful-status split this feature exists to prove.
  - The live run genuinely FAILED on first try — proving the harness works: `parity_build`, `parity_corpus_tier1`, `parity_corpus_merged` surfaced real, previously-invisible divergences (the tests had silently skipped since the feature's inception because no oracle/Docker was ever available in any prior CI run). Triaged each (see PR #306 discussion):
    - `extends-child` (deacon succeeds, reference fails on `--include-merged-configuration`) — an already-documented, intentional divergence (REPORT.md, issue #297) that the harness had no way to waive (process-exit mismatches weren't wired to the waiver system, only value divergences were). **Fixed in-PR**: new `Expect::ReferenceStricter` waiver variant + `corpus_runner` wiring + `waivers/extends-child-merged.json` record — this was a genuine harness completeness gap, squarely in scope.
    - Four genuine, unrelated deacon product bugs newly surfaced (out of scope for this harness-hardening PR per spec's Out-of-Scope guidance) — filed as follow-ups, not fixed or silently waived: #307 (image-metadata dropped when image not locally pulled, ~19/23 corpus cases), #308 (`hostRequirements.cpus` serializes as float), #309 (`${containerWorkspaceFolder}` echoes host path instead of `/workspaces/<name>`), #310 (`build` JSON `imageName` not an array for the single-tag case).
  - Re-running the certification lane after the extends-child fix (to confirm only #307-#310 remain, all correctly characterized as out-of-scope follow-ups) is tracked as a live status check on PR #306, not a blocking gate for this task — the fail-fast/no-silent-skip proof (the actual acceptance criterion) is already established.

---

## Deferred Work

None. All research decisions (D1–D12) are fully implemented by the tasks above; no phased deferrals were taken. (Future oracle upgrades and any snapshot/replay mode are out of scope per spec, not deferrals.)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 → Phase 2**: crate must exist before modules land.
- **Phase 2 blocks all stories** (harness core is the shared dependency).
- **US1 (Phase 3)** first: it owns the nextest restructure (T013) that every later binary task assumes, and migrating the six binaries frees `parity_utils.rs` for deletion (T020).
- **US2 (Phase 4)** depends on US1's profile + `parity_utils` deletion (T024 builds on T018/T019 state-normalization move).
- **US3 (Phase 5)** depends on fragments existing (US1) and the registry (T025/T026 in US2) for the aggregator's completeness gate.
- **US4 (Phase 6)** verification depends only on Phase 2 + T030 (Python deletion) — can run in parallel with US3.
- **US5 (Phase 7)** depends on US2 (registry, waivers, renames) and the final profile shape (T032); T043/T044 depend only on Phase 2.
- **Phase 8** last (T051 needs the workflow from T037).

### Key task-level dependencies

- T013 with T014–T019 land together (keep-green constraint, constitution III)
- T020 requires T014–T019 complete; T024 requires T018/T019 + T022
- T027–T029 require T022 (waivers), T025/T026 (registry minimums), Phase 2 normalize/exec
- T032 requires T027–T029 + T031; T034 requires T025/T026; T037 requires T034 + T036
- T045 requires T026 + T031 + T032 (final registry + profile shape)

### Parallel Opportunities

- Phase 2: T006 ∥ T005; T010 ∥ T011 after their modules
- US1: T015–T019 all parallel after T014 establishes the migration pattern (different files)
- US2: T023 ∥ T022-dev; T028 ∥ T029 after T027; T031 anytime in-phase
- US3: T035 ∥ T036 after T034
- US4: T040 ∥ T042; whole phase ∥ US3/US5 (different files)
- US5: T044 ∥ T045 after T043
- Polish: T048 ∥ T049

---

## Implementation Strategy

**MVP = Phases 1–3 (US1)**: after T021, a green parity result already implies a verified 0.87.0 oracle and a real comparison for all six existing scenario binaries — the core trust property — while every other lane stays green by honest non-selection. Stop, validate, ship.

**Increment 2 (US2)**: corpus runners + waivers + registry → whole surface under one contract. **Increment 3 (US3)**: aggregator + certification lane → CI-proof. **Increments 4–5 (US4/US5)**: consolidation proofs and the fault-injection guard (US4 and US5's stub-based tasks can proceed in parallel with US3 if staffed). Each increment leaves `main`-mergeable green state.

**Keep-green cadence** (constitution III): fmt + clippy + `make test-nextest-fast` after every task; explicit gate tasks (T012, T021, T033, T039, T047, T050) mark phase boundaries. Live-oracle verification happens locally where possible and definitively in T051's certification-lane run.

---

## Notes

- Format check: all 51 tasks use `- [ ] Txxx [P?] [Story?] description + path(s)`; setup/foundational/polish carry no story label; every story task carries US1–US5.
- The repo-wide sweep tasks (T020, T038, T041) are the anti-regression teeth — do not skip them even when the obvious call sites are already migrated.
- When editing `.config/nextest.toml`, remember conflicts resolve to the UNION of `binary(=…)` clauses (CLAUDE.md) — relevant if other PRs land test binaries concurrently.
