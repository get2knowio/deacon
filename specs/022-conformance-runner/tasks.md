---
description: "Task list for Declarative Conformance Runner (022-conformance-runner)"
---

# Tasks: Declarative Conformance Runner

**Input**: Design documents from `/specs/022-conformance-runner/`
**Prerequisites**: plan.md, spec.md, research.md (D1–D10), data-model.md, contracts/ (case-schema, snapshot-provenance, observer-channel, runner-cli)

**Tests**: INCLUDED — acceptance tests are mandatory for this feature (spec SC-012 + Constitution VII "Testing Completeness"). Hermetic tests run in `dev-fast`/`default`; the single live binary runs only under `--profile parity`; Docker channel tests use docker resource groups.

**Organization**: Grouped by user story (US1–US6) in priority order (P1 → P2 → P3). Each story is independently testable.

## Path conventions

- Hermetic data/validation/staleness → `crates/conformance/` (`deacon-conformance`)
- Live execution/observation/record → `crates/parity-harness/`
- Registry data → `conformance/registry/`, committed snapshots → `conformance/snapshots/`
- Live test binary → `crates/deacon/tests/parity_conformance_runner.rs`
- Wiring → `.config/nextest.toml`, `fixtures/parity-corpus/registry.json`

**Green-build cadence (Constitution III)**: after every task run `cargo fmt --all && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings`, then the narrowest relevant `make test-nextest-*`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create module scaffolding + registry data so the workspace compiles and new binaries are discoverable.

- [X] T001 Create stub modules and register them so the workspace builds: `crates/conformance/src/case_hash.rs`, `crates/conformance/src/snapshot.rs` (declared in `crates/conformance/src/lib.rs`); `crates/parity-harness/src/{runner.rs,evidence.rs,compare.rs,oracle_type.rs,workspace.rs}` and `crates/parity-harness/src/observe/mod.rs` with per-channel stub files `observe/{cli_process,filesystem,image,container_graph,injected_process,temporal}.rs` (declared in `crates/parity-harness/src/lib.rs`)
- [X] T002 [P] Extend `conformance/registry/channels.json` with five new channel records: `chan-structured-output`, `chan-image`, `chan-process-graph`, `chan-injected-process`, `chan-temporal` (descriptions per data-model.md §4)
- [X] T003 [P] Add empty hermetic test-binary stubs `crates/conformance/tests/{case_schema_valid,snapshot_staleness,allowed_difference_scoping,normalization_semantics}.rs` and `crates/parity-harness/tests/runner_record_replay.rs` so nextest lists them
- [X] T004 [P] Add these hermetic test binaries to `.config/nextest.toml` (they run in `default`/`dev-fast`; no docker group), following the existing conformance-test override pattern

**Checkpoint**: `cargo build` and `cargo nextest list` succeed with all new modules/binaries present (stubbed).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types, hashing, base loader/validation, evidence/verdict scaffolding, observer contract, and the normalization entry point — everything every story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T005 Add core declarative types to `crates/conformance/src/model.rs`: `OracleType` enum (4 variants), `Operation`, `Fixture`, `ExpectedObservable`, `Channel` id constants, `Cleanup`, `ResourceGroup` enum, and the declarative fields on the `Case` record (`operations`, `oracleType`, `expected`, `allowedDifferences` (typed later), `fsAllowlist`, `cleanup`, `resourceGroup`, `notes`) per data-model.md §1–§5
- [X] T006 [P] Implement `caseHash` (behavior-affecting inputs only) and `fixtureHash` (fixture bytes) in `crates/conformance/src/case_hash.rs` reusing the `sha2::{Digest,Sha256}` pattern from `crates/conformance/src/clause.rs` (research D3)
- [X] T007 Extend the loader in `crates/conformance/src/load.rs` to parse legacy vs declarative case records (mutually exclusive) and reject a mixed/neither record with a located, fail-loud error (FR-003)
- [X] T008 Add the closed `FailurePhase` enum (config-resolution, build, container-create, lifecycle:onCreate/updateContent/postCreate/postStart/postAttach, exec) to `crates/conformance/src/model.rs` (data-model.md §8, clarify Q5)
- [X] T009 [P] Add evidence/verdict base types to `crates/parity-harness/src/evidence.rs`: `RawChannelEvidence`/`NormalizedChannelEvidence` (with `present: bool` distinguishing not-captured from empty, FR-018), `ChannelVerdict`, `CaseVerdict`, and the `outcome` enum (agree/diverge/allowed-difference/no-reference-for-platform/stale/error)
- [X] T010 [P] Define the `ChannelObserver` contract and `RunContext` in `crates/parity-harness/src/observe/mod.rs` (`capture(ctx, op) -> Result<RawChannelEvidence, HarnessError>`, contracts/observer-channel.md)
- [X] T011 [P] Add the single normalization entry point + `NORMALIZER_VERSION` constant to `crates/parity-harness/src/normalize.rs` as a pass-through (real named rules land in US3); expose `normalize_channel(channel, raw) -> NormalizedChannelEvidence`
- [X] T012 [P] Extend `HarnessError` in `crates/parity-harness/src/lib.rs` with cause-specific variants: `DockerUnavailable`, `NodeUnavailable`, `NormalizationFailed`, `SnapshotStale{field}`, `NoReferenceForPlatform{os_arch}` (Constitution IV fail-loud)
- [X] T013 Add core case-well-formedness validation classes to `crates/conformance/src/validate.rs` (continue the V-series): exactly-one-of legacy/declarative, `oracleType` enum, each `operations[].subcommand` in the consumer surface (Principle II), `spec-expectation` ⇒ every `expected.assertion` present, `fsAllowlist` present iff a filesystem-channel expectation exists
- [X] T014 [P] Extend the conformance CLI in `crates/conformance/src/bin/conformance.rs` with a `Snapshot` subcommand group skeleton (`check`, `diff`) wired to stub handlers (implemented in US2)

**Checkpoint**: Core types + hashing + base validation compile and are unit-referenced; `cargo run -p deacon-conformance -- validate` runs (new classes active) against the existing registry without regressions.

---

## Phase 3: User Story 1 - Author and run a declarative case against a selectable oracle (Priority: P1) 🎯 MVP

**Goal**: A declarative case runs against deacon (spec-expectation) and, in the parity lane, against the pinned reference (live-differential), producing per-channel CLI-process verdicts — with no new Rust test function per case.

**Independent Test**: Register a `read-configuration` case as data with expected CLI-process/structured-output observables; `cargo nextest run -p deacon-conformance` proves the hermetic spec-expectation verdict; a malformed case fails loudly at load.

### Tests for User Story 1

- [X] T015 [P] [US1] In `crates/conformance/tests/case_schema_valid.rs`: assert a well-formed declarative case loads; unknown behavior, undeclared channel, legacy+declarative mix, and a non-consumer subcommand each fail loudly with a located message (FR-001..004, FR-003)
- [X] T016 [P] [US1] In `crates/parity-harness/tests/runner_record_replay.rs` (spec-expectation section): a fixture case with an intentional wrong exit-code expectation yields a `diverge` verdict; a correct one yields `agree` (FR-015)
- [X] T017 [P] [US1] Add a CLI-process failure-phase test: a case whose op fails records partial evidence + the correct `FailurePhase` from the closed set (spec Edge "Partial capture", FR-009)
- [X] T018 [P] [US1] Add a report-determinism test asserting the verdict report is byte-stable (declaration order, no timestamps/absolute paths) per contracts/runner-cli.md

### Implementation for User Story 1

- [X] T019 [P] [US1] Implement the `cli_process` observer in `crates/parity-harness/src/observe/cli_process.rs` (exit code, stdout, stderr, failure phase) reusing `exec.rs::Invocation` capture
- [X] T020 [P] [US1] Implement the `structured_output` capture path in `crates/parity-harness/src/observe/cli_process.rs` (parsed JSON result doc → `chan-structured-output`) reusing `Invocation::stdout_json`
- [X] T021 [US1] Implement per-channel comparison in `crates/parity-harness/src/compare.rs` for CLI-process/structured-output channels producing `ChannelVerdict`s (allowed-difference integration deferred to US4)
- [X] T022 [US1] Implement `spec-expectation` and `live-differential` dispatch in `crates/parity-harness/src/oracle_type.rs` (live-differential runs deacon + the verified pinned oracle via `oracle.rs`; snapshot + invariant deferred to US2/US6)
- [X] T023 [US1] Implement the runner orchestration in `crates/parity-harness/src/runner.rs`: load case → run operations against the target → invoke declared observers → normalize (pass-through) → compare → emit `CaseVerdict` (fail-loud on missing oracle)
- [X] T024 [US1] Implement the deterministic verdict report writer (single JSON on stdout, `tracing` logs on stderr, ordered records) in `crates/parity-harness/src/report.rs` per contracts/runner-cli.md
- [X] T025 [US1] Add a hermetic spec-expectation fixture case to `conformance/registry/cases.json` (read-configuration) + its fixture files, mirroring quickstart.md §1
- [X] T026 [US1] Create the live test binary `crates/deacon/tests/parity_conformance_runner.rs` (thin shell driving the runner over declarative cases; fail-loud on missing prereqs, no `#[ignore]`)
- [X] T027 [US1] Register `parity_conformance_runner` in `fixtures/parity-corpus/registry.json` AND add nextest overrides in ALL profiles per the CLAUDE.md 3-spot rule (parity `default-filter` allow-list + `dev-fast` default-filter exclusion + `dev-fast`/`default`/`full`/`ci`/`mvp-integration` override exclusions), so `parity_registry_check` stays green

**Checkpoint**: US1 fully functional — a data-only case runs and verdicts in dev-fast (spec-expectation) and in the parity lane (live-differential). MVP complete.

---

## Phase 4: User Story 2 - Record and replay provenance-tracked snapshots with staleness gating (Priority: P2)

**Goal**: Committed, os-arch-keyed snapshots with 13-field provenance; replay fails as stale on any drift; refresh is a reviewed-only action; ordinary runs never write.

**Independent Test**: Record a snapshot; `snapshot check` passes; edit an operation → `snapshot check` fails as stale naming `caseHash`; editing only `notes` does not; an ordinary run writes nothing.

### Tests for User Story 2

- [X] T028 [P] [US2] In `crates/conformance/tests/snapshot_staleness.rs`: each of `caseHash/fixtureHash/oracleVersion/sourceRevision/nodeVersion/dockerVersion/composeVersion/imageDigests/normalizerVersion` drift makes `check` fail as stale naming the field; `capturedAt`/`platform`/`arch` do not (FR-020, SC-003)
- [X] T029 [P] [US2] Assert a missing snapshot for the current `os-arch` yields `no-reference-for-platform` (distinct from stale and from skip) (FR-016a, clarify Q3)
- [X] T030 [P] [US2] In `crates/parity-harness/tests/runner_record_replay.rs` (replay section): a recorded case replays to the same verdict as its recording (record/replay equivalence) and provenance carries all 13 fields (SC-011, SC-002)

### Implementation for User Story 2

- [X] T031 [P] [US2] Implement `Provenance` + `Snapshot` model and load in `crates/conformance/src/snapshot.rs` (13 fields, os-arch key, raw/normalized separate) per contracts/snapshot-provenance.md
- [X] T032 [US2] Implement pure staleness comparison in `crates/conformance/src/snapshot.rs` (recorded vs recomputed hashes + probed env; name first mismatch; platform/arch as selectors, not staleness) (FR-020, D5)
- [X] T033 [US2] Implement `snapshot check` and `snapshot diff` handlers in `crates/conformance/src/bin/conformance.rs` (hermetic; exit codes per contracts/runner-cli.md); add snapshot provenance validation classes to `crates/conformance/src/validate.rs`
- [X] T034 [P] [US2] Implement atomic evidence writes (temp file + `fs::rename`) for `provenance.json`/`raw.json`/`normalized.json` in `crates/parity-harness/src/evidence.rs`, with a unit test covering the temp-file+rename path so a shorter payload never leaves trailing bytes (FR-019, D4)
- [X] T035 [US2] Implement the environment probe (oracle version via `oracle.rs`, Node/Docker/Compose versions, image digests) in `crates/parity-harness/src/runner.rs` for provenance capture
- [X] T036 [US2] Add `snapshot` oracle-type dispatch in `crates/parity-harness/src/oracle_type.rs` (compare normalized evidence to the provenance-checked committed snapshot; emit `stale`/`no-reference-for-platform`)
- [X] T037 [US2] Create the reviewed refresh bin `crates/parity-harness/src/bin/conformance-snapshot.rs` (`refresh [--case] [--platform]`; requires verified oracle + Docker/Node fail-loud; writes atomically; prints review diff) per contracts/runner-cli.md
- [X] T038 [US2] Add a snapshot-oracle fixture case + its committed `conformance/snapshots/<os>-<arch>/<case-id>/` evidence recorded via the refresh bin; assert ordinary `cargo nextest run` never rewrites it (FR-021, SC-004). **Record only after US3 finalizes `NORMALIZER_VERSION`** (see Ordering constraint) — otherwise the committed snapshot is immediately stale.

**Checkpoint**: US2 works independently — record, replay, staleness gating, and reviewed-only refresh.

---

## Phase 5: User Story 3 - Normalize evidence semantically, storing raw and normalized separately (Priority: P2)

**Goal**: Named, field-specific normalization rules replace the pass-through; raw and normalized evidence persisted separately; null/empty/default preserved; nothing blanket-removed.

**Independent Test**: Evidence with a temp path, null/empty/default fields, a label, a mount source, and a PATH normalizes to a `<WORKSPACE>` token with the four null states intact and no blanket removals.

### Tests for User Story 3

- [X] T039 [P] [US3] In `crates/conformance/tests/normalization_semantics.rs` (or a parity-harness normalize test): temp path → stable token (not deleted); missing/null/empty/defaulted stay distinct; no env/label/mount/entrypoint/command/network blanket-removed (FR-024/025/029, SC-007)
- [X] T040 [P] [US3] Assert `label_semantic` parses labels to key/value and compares semantically; `mount_source_canonical` makes two mounts differing only by temp path compare equal; `path_env_segmented` compares PATH segment-wise (FR-026/027/028)
- [X] T041 [P] [US3] Assert raw and normalized evidence are persisted separately and independently retrievable for a run (FR-016, SC-006); assert a not-captured channel (`present:false`) stays distinguishable from a captured-but-empty value (`present:true`, empty/null) (FR-018)

### Implementation for User Story 3

- [X] T042 [US3] Implement the named rules in `crates/parity-harness/src/normalize.rs`: `path_token`, `label_semantic`, `mount_source_canonical`, `path_env_segmented` (segment-wise PATH **plus the optional executable probe where a rule requires it**, FR-028), `null_preserving`; reclassify the existing `NOISE_ENV_KEYS`/`INTENTIONAL_LABEL_PREFIXES` as named, scoped, rationale-carrying rules (research D6, FR-023/029)
- [X] T043 [US3] Bump `NORMALIZER_VERSION` and wire it into provenance + staleness (FR-030); replace the T011 pass-through with per-channel rule application
- [X] T044 [P] [US3] Implement the `filesystem` observer in `crates/parity-harness/src/observe/filesystem.rs` (allowlist-scoped capture rooted at workspace/out-dir, NOT full-tree) applying `path_token` (FR-010, clarify Q1)
- [X] T045 [US3] Persist raw + normalized evidence separately in `crates/parity-harness/src/evidence.rs` and thread through the runner (FR-016)
- [X] T046 [P] [US3] Add a filesystem-channel fixture case to `conformance/registry/cases.json` with an `fsAllowlist` and expected file effects
- [X] T047 [US3] Ensure `compare.rs` operates on normalized evidence for stdout/stderr/structured-output/filesystem channels (path-tokenized comparison)
- [X] T048 [US3] Update `crates/conformance/src/report.rs`/`coverage.rs` so the conformance `report` surfaces normalized-evidence coverage per channel

**Checkpoint**: US3 works — faithful normalization + separate raw/normalized persistence; US1/US2 comparisons now use real rules.

---

## Phase 6: User Story 5 - Run Docker-backed cases in isolated workspaces with reliable cleanup (Priority: P2)

**Goal**: Docker cases run in isolated external temp workspaces with collision-resistant names, pinned inputs, guaranteed cleanup on success/failure, and correct resource groups — enabling the image/graph/injected-process/temporal channels.

**Independent Test**: Run a Docker case to success and to forced failure; both leave zero residual containers/images/networks/volumes/temp dirs; two concurrent cases don't collide on names.

### Tests for User Story 5

- [X] T049 [P] [US5] Add a docker-group cleanup test asserting zero residual resources after a Docker case on BOTH success and forced-failure/interruption (FR-039, SC-009); assert via `docker run <tag> cat <marker>` / `docker ps -a` sweeps, not just JSON outcome
- [X] T050 [P] [US5] Add a collision test: two Docker cases concurrently allocate non-colliding container/network/volume names (FR-037, SC-010)
- [X] T051 [P] [US5] Add per-channel capture tests for `chan-image` (labels), `chan-process-graph` (mounts/networks/volumes), `chan-injected-process` (env/user/cwd/PATH/TTY), `chan-temporal` (lifecycle order, first-create vs restart, cleanup) (SC-005)

### Implementation for User Story 5

- [X] T052 [US5] Implement `crates/parity-harness/src/workspace.rs`: isolated external temp workspace (`tempfile`), collision-resistant resource names, and an RAII cleanup guard that reclaims resources on success AND unwind (FR-036/037/039)
- [X] T053 [P] [US5] Implement the `image` observer in `crates/parity-harness/src/observe/image.rs` (built-image config + labels via `label_semantic`) (FR-011)
- [X] T054 [P] [US5] Implement the `container_graph` observer in `crates/parity-harness/src/observe/container_graph.rs` (container/network/volume/mount graph via `mount_source_canonical`) (FR-012)
- [X] T055 [P] [US5] Implement the `injected_process` observer in `crates/parity-harness/src/observe/injected_process.rs` (env, user, cwd, PATH via `path_env_segmented` including executable-probe resolution where a rule requires it (FR-028), signals, TTY, exit propagation) (FR-013)
- [X] T056 [P] [US5] Implement the `temporal` observer in `crates/parity-harness/src/observe/temporal.rs` (lifecycle ordering, first-create vs restart, resume, cleanup transitions) (FR-014)
- [X] T057 [US5] Wire `resourceGroup` from the case into scheduling and add nextest overrides for the Docker channel tests in ALL profiles (`docker-shared` where names are unique, `docker-exclusive` only where state is shared) per Constitution VII / CLAUDE.md
- [X] T058 [US5] Add pinned-input enforcement (reject `latest`, require digest/tag) at case load for Docker cases in `crates/conformance/src/validate.rs` (FR-038)
- [X] T059 [US5] Add Docker-backed fixture cases to `conformance/registry/cases.json` (up/exec) exercising image/graph/injected/temporal channels; `#[cfg(unix)]`-gate host-hook/Docker-only assertions with a one-line reason (cross-platform convention)

**Checkpoint**: US5 works — Docker isolation, guaranteed cleanup, and the four Docker-requiring channels.

---

## Phase 7: User Story 4 - Scope allowed differences to a behavior and a waiver identity (Priority: P3)

**Goal**: Tolerated divergences are scoped to `(behavior, context, observablePath)` + a resolvable waiver/divergence id; no global ignore lists; conflicts rejected at load; self-invalidating.

**Independent Test**: A scoped allowed-difference lets its `(behavior, path)` pass while the same difference on another path still fails; a conflicting duplicate fails at load; a global ignore list is rejected.

### Tests for User Story 4

- [X] T060 [P] [US4] In `crates/conformance/tests/allowed_difference_scoping.rs`: a scoped difference tolerates only its `(behavior, observablePath, context)`; the same difference on path B still fails; a conflicting duplicate and a global-ignore-style construct both fail at load (FR-031/032/033/035, SC-008)
- [X] T061 [P] [US4] Assert an allowed difference whose backing `wvr-`/divergence no longer reproduces is reported stale, reusing the registry's self-invalidating waiver check (FR-034)

### Implementation for User Story 4

- [X] T062 [US4] Implement the typed `AllowedDifference` model + parsing in `crates/conformance/src/model.rs`/`load.rs` (behavior, context, observablePath, rationale, waiverId|divergenceId) per data-model.md §6
- [X] T063 [US4] Add allowed-difference validation to `crates/conformance/src/validate.rs`: dangling waiver/divergence id, `(behavior, observablePath)` conflict, bare-channel/global-ignore rejection (FR-031/032/035)
- [X] T064 [US4] Integrate allowed differences into `crates/parity-harness/src/compare.rs`: a covered divergence becomes `allowed-difference` (with waiver id in detail); uncovered stays `diverge` (FR-033)
- [X] T065 [US4] Resolve `waiverId`/`divergenceId` against `conformance/registry/waivers/`/`ext-` records reusing `parity-harness/src/waiver.rs` (no parallel mechanism, FR-043); surface stale allowed-differences in the report

**Checkpoint**: US4 works — honest, scoped tolerance with no global ignore lists.

---

## Phase 8: User Story 6 - Choose the oracle type per case (Priority: P3)

**Goal**: All four oracle types are explicit and distinct; invariant/metamorphic evaluates a declared relationship across operations; a case can be re-pointed by changing only `oracleType`.

**Independent Test**: One case evaluated under each oracle type applies distinct semantics; a metamorphic case checks idempotence/first-create-vs-restart/resume rather than a fixed output.

### Tests for User Story 6

- [X] T066 [P] [US6] Add a test evaluating one fixture case under all four oracle types, asserting distinct verdict semantics and that re-pointing changes only `oracleType` (FR-006/007)
- [X] T067 [P] [US6] Add a metamorphic test: an idempotence relationship (re-`up`) and a first-create-vs-restart relationship each verdict on the declared relationship, not a fixed output (FR-008)

### Implementation for User Story 6

- [X] T068 [US6] Finalize explicit four-way dispatch in `crates/parity-harness/src/oracle_type.rs` (spec-expectation, snapshot, live-differential, invariant-metamorphic) with a single entry consumed by the runner
- [X] T069 [US6] Implement invariant/metamorphic evaluation (relationship kinds idempotence, first-create-vs-restart, resume across ≥2 operations) in `crates/parity-harness/src/oracle_type.rs` (FR-008)
- [X] T070 [US6] Add oracle-type arity validation to `crates/conformance/src/validate.rs` (invariant-metamorphic ⇒ ≥2 ops + a `relationship` referencing a sibling op id) per data-model.md §1
- [X] T071 [US6] Add a metamorphic fixture case to `conformance/registry/cases.json` (idempotent re-`up`) using the `chan-temporal` channel

**Checkpoint**: All six user stories independently functional.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, gate wiring, and end-to-end validation across stories.

- [X] T072 [P] Update `CLAUDE.md` (Conformance Registry + Parity sections) to document the declarative case shape, snapshot tree, and the runner/refresh split
- [X] T073 Ensure `certify` (`crates/conformance/src/certify.rs`) surfaces snapshot coverage + `no-reference-for-platform` as non-blocking info and still blocks on gaps/unclassified (FR-042, release-gate integrity)
- [X] T074 [P] Add a section to `specs/018-harden-parity-harness/quickstart.md`-style docs or `conformance/RULES.md` noting the runner is dev-only (no shipped `deacon` subcommand, Principle II)
- [X] T075 Run `cargo nextest run --profile parity -E 'binary(=parity_conformance_runner)'` locally (needs Docker + pinned oracle) to confirm the live lane is green; rely on the `parity / live-certification` CI lane otherwise
- [X] T076 [P] Run the quickstart.md end-to-end (author → validate → run → record → staleness → characterize) and fix any drift
- [X] T077 Full gate before PR: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`, and confirm `parity_registry_check` + `registry_valid` pass

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (P1)**: no dependencies.
- **Foundational (P2)**: depends on Setup — BLOCKS all user stories.
- **US1 (P1)**: depends on Foundational. MVP.
- **US2 / US3 / US5 (P2)**: depend on Foundational; independently testable. US2's meaningful *normalized* snapshots and US5's *normalized* channel comparisons are sharper after US3 lands, but each story's independent test does not require US3 (US2 staleness is provenance/hash-based; US5 can assert on raw capture).
  - **Ordering constraint (normalizer version)**: US3 finalizes `NORMALIZER_VERSION`. Any snapshot recorded in US2 (T038) *before* US3 lands is invalidated as stale on the next run (normalizerVersion drift, FR-030). Therefore committed US2 snapshots MUST be recorded only after US3's normalizer is final — the single-track order below enforces this by running **US3 before US2**. If US2 is implemented first, its snapshots MUST be re-recorded after US3.
- **US4 / US6 (P3)**: depend on Foundational; US4 integrates into `compare.rs` (US1), US6 into `oracle_type.rs` (US1/US2) — both add isolated logic and are independently testable.
- **Polish (P9)**: depends on all targeted stories.

### Recommended order (single track)

Setup → Foundational → **US1 (MVP)** → US3 → US2 → US5 → US4 → US6 → Polish.
(US3 pulled ahead of US2/US5 among the P2s so normalization is real before snapshots/Docker channels rely on it — reduces rework.)

### Within each story

Tests first (must fail) → models → observers/services → runner/dispatch integration → fixtures/wiring.

---

## Parallel Opportunities

- **Setup**: T002, T003, T004 in parallel (distinct files) after T001.
- **Foundational**: T006, T009, T010, T011, T012, T014 in parallel (distinct files); T005/T007/T013 touch shared `model.rs`/`load.rs`/`validate.rs` — serialize those.
- **US1**: tests T015–T018 in parallel; impl T019/T020 in parallel; T021–T024 serialize (shared `compare`/`oracle_type`/`runner`/`report`).
- **US3**: tests T039–T041 parallel; T044 parallel with T042/T043.
- **US5**: tests T049–T051 parallel; observers T053–T056 fully parallel (distinct files).
- **US4/US6**: tests parallel; implementation mostly serial within each story.
- **Cross-story**: once Foundational is done, US1/US3/US2/US5 can be staffed in parallel by different developers (distinct primary files), integrating through `runner.rs`/`compare.rs`/`oracle_type.rs`.

### Parallel example: User Story 5 observers

```bash
Task: "Implement image observer in crates/parity-harness/src/observe/image.rs"
Task: "Implement container_graph observer in crates/parity-harness/src/observe/container_graph.rs"
Task: "Implement injected_process observer in crates/parity-harness/src/observe/injected_process.rs"
Task: "Implement temporal observer in crates/parity-harness/src/observe/temporal.rs"
```

---

## Implementation Strategy

### MVP first (US1 only)

1. Setup → Foundational → US1.
2. **STOP & VALIDATE**: a data-only `read-configuration` case verdicts in dev-fast (spec-expectation) and in the parity lane (live-differential), with zero new Rust test functions per case (SC-001).
3. Demo: adding a second case is a pure `cases.json` edit.

### Incremental delivery

US1 (author+run) → US3 (real normalization) → US2 (snapshots+staleness) → US5 (Docker channels+cleanup) → US4 (scoped allowed diffs) → US6 (oracle types). Each adds value without breaking prior stories; `parity_registry_check`/`registry_valid` stay green throughout.

### SC → task coverage map (SC-012 acceptance areas)

| Acceptance area (SC-012) | Task(s) |
|--------------------------|---------|
| Each observable channel | T017/T019/T020 (CLI), T044/T046 (fs), T051/T053–T056 (image/graph/injected/temporal), T020 (structured) |
| Raw vs normalized evidence | T041, T045 |
| Stale snapshots | T028, T032 |
| Canonicalization | T039, T042 |
| Metadata labels | T040, T053 |
| Mounts | T040, T054 |
| PATH | T040, T055 |
| Null semantics | T039, T042 |
| Allowed-difference scoping | T060, T063, T064 |
| Process failures | T017, T019 |
| Resource cleanup | T049, T052 |
| Record/replay equivalence | T030 |

---

## Notes

- `[P]` = different files, no incomplete-task dependency.
- `[USx]` maps each task to its user story for traceability.
- Every task keeps the build green (fmt + clippy + narrowest `make test-nextest-*`); full gate (T077) before PR.
- The runner is **dev-only** — never add a shipped `deacon` subcommand (Constitution II).
- Reuse, don't fork: one normalizer (`normalize.rs`), one waiver mechanism (`waiver.rs` + registry), one loader (`deacon-conformance`) — Constitution VIII.
- Each build-out step is its own small CI-gated PR (Conventional-Commit `feat`/`fix`/`chore`, never `test`/`style`).
