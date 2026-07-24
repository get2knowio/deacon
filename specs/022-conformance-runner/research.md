# Phase 0 Research: Declarative Conformance Runner

**Feature**: 022-conformance-runner | **Date**: 2026-07-24

The spec's Technical Context has no open `NEEDS CLARIFICATION` markers — the five clarify-session answers plus the fixed toolchain (Edition 2024 / MSRV 1.95, existing workspace deps) resolve the stack. The research below records the design *decisions* that shape Phase 1, each grounded in the existing `deacon-conformance` and `parity-harness` machinery so the build-out reuses rather than reinvents (Constitution VIII).

---

## D1. Crate placement — extend two existing crates, add none

- **Decision**: Hermetic data/validation/staleness/pure-normalization-rule logic lands in `crates/conformance` (`deacon-conformance`); live execution/observation/record logic lands in `crates/parity-harness`. No new crate.
- **Rationale**: `parity-harness` already depends on `deacon-conformance` and already owns `exec`/`normalize`/`oracle`/`report`/`waiver`. The dependency direction forces the split: the case *schema* and *loader* must live in `deacon-conformance` (so `validate`/`certify`/`report` see it), while *executing* a case needs Docker/Node, which is `parity-harness` territory. A third crate would duplicate loaders and violate Principle VIII.
- **Alternatives considered**: (a) A new `conformance-runner` crate — rejected: re-exposes the registry loader and normalizer, inviting drift. (b) All in `parity-harness` — rejected: hermetic validation must run in `dev-fast`/PR lanes without Docker, and `certify` (release gate) lives in `deacon-conformance`.

## D2. Case schema — extend `cases.json`, coexist with legacy binary-backed cases

- **Decision**: Extend the existing `cases.json` record (`id`, `behaviors[]`, `context[]`, `executable`, `outcomes[]`) with a declarative block: `operations[]`, `oracleType`, `expected` (per-channel), `allowedDifferences[]`, `cleanup`, `resourceGroup`, `fsAllowlist[]`. A record is either **legacy** (`executable.binary` → a Rust test) or **declarative** (`operations` present); the loader accepts both, and validation forbids mixing.
- **Rationale**: `cases.json` is already loaded and gated by V3/V9/V10. Extending it means the new cases inherit behavior-linkage validation, coverage counting, and `certify` for free. Coexistence lets legacy `parity_*` binaries migrate incrementally instead of a big-bang rewrite (Constitution I phased implementation).
- **Alternatives considered**: A separate `case-specs/` directory — rejected: fragments the coverage/certify view across two files; the registry's whole value is one authoritative `cases.json`.

## D3. Case hash & fixture hash — reuse `sha2`, hash behavior-affecting inputs only

- **Decision**: Compute `caseHash` as SHA-256 over the **canonicalized behavior-affecting inputs** (operations, argv, oracle type, referenced fixture hashes) — excluding `rationale`/`notes`/`allowedDifferences` prose. `fixtureHash` is SHA-256 over the fixture file bytes (sorted, path-relative). Reuse the `sha2::{Digest, Sha256}` + `hash8` pattern already in `crates/conformance/src/clause.rs`.
- **Rationale**: Matches the clarify answer (Q4) — annotating a case must not force re-recording. Mirrors the substance-anchored clause-id design (location/prose excluded from the hash) already proven in 021.
- **Alternatives considered**: Hash the whole record — rejected: every prose edit invalidates snapshots, defeating reviewable annotation. Content-addressed fixtures via git object ids — rejected: adds a git dependency to a pure-data hash.

## D4. Snapshot layout & provenance — committed, os-arch-keyed, atomic

- **Decision**: `conformance/snapshots/<os>-<arch>/<case-id>/` holds `provenance.json`, `raw.json`, `normalized.json` (raw and normalized **separate**, per FR-016). `provenance.json` carries the 13 fields of FR-017. Writes use the temp-file + `fs::rename` atomic pattern (`crates/core/src/cache/disk.rs::save_index`, already the harness convention). Snapshots are committed and reviewed as ordinary registry changes.
- **Rationale**: Clarify answers Q2 (committed → reviewable diff) and Q3 (os-arch keyed). Committed storage is the *only* way FR-022's "refresh surfaces a reviewable diff" works. Atomic writes satisfy FR-019 and the "concurrent refresh must not corrupt" edge case.
- **Alternatives considered**: `target/` (ephemeral) — rejected: a refresh would leave no reviewable diff and replay would be non-hermetic. A single combined evidence file — rejected: FR-016 mandates raw/normalized separation.

## D5. Staleness detection — pure comparison in `deacon-conformance`, live re-record in `parity-harness`

- **Decision**: Staleness is a **pure** comparison (recorded provenance/hashes vs current inputs + current environment probe) implemented in `deacon-conformance::snapshot` and exposed via `conformance snapshot check`; it names the first mismatched field and fails (FR-020). Live *re-recording* (running the oracle to regenerate evidence) is `parity-harness`'s `conformance-snapshot` bin, run only as a reviewed action. A missing snapshot for the current `os-arch` yields a distinct **"no reference for platform"** verdict — not stale, not a silent skip (FR-016a).
- **Rationale**: Separates the hermetic gate (can run in `dev-fast`/PR, no Docker) from the Docker/Node record path. Reuses the existing `oracle.rs` version verification for the `oracleVersion` field and a lightweight env probe for Docker/Compose/Node versions.
- **Alternatives considered**: Fold record + check into one Docker-only tool — rejected: staleness must be enforceable in the hermetic PR lane, else rot ships silently.

## D6. Normalization — extend the single `normalize.rs`, never fork it

- **Decision**: Add **named, field-specific rules** to the existing `crates/parity-harness/src/normalize.rs` (not a new module): `path_token` (rewrite temp workspace/project paths → stable tokens, FR-024), `label_semantic` (parse metadata labels, FR-026), `mount_source_canonical` (path-substitute mount sources before compare, FR-027), `path_env_segmented` (segment-wise PATH + optional executable probe, FR-028), and `null_preserving` (keep missing/null/empty/default distinct, FR-025). Bump a `NORMALIZER_VERSION` constant recorded in provenance and used in staleness (FR-030).
- **Rationale**: Constitution VIII — there is exactly one normalizer. The current `normalize.rs` already has `NOISE_ENV_KEYS`/`INTENTIONAL_LABEL_PREFIXES` and `container_state`; these are refactored so nothing is *blanket-removed* (FR-029) — noise keys become named, scoped rules with recorded rationale.
- **Risk & mitigation**: The existing `NOISE_ENV_KEYS`/`INTENTIONAL_LABEL_PREFIXES` are close to the "blanket remove" the spec forbids. Mitigation: reclassify each as a *named rule with a rationale*, scoped to a channel/field, covered by `normalization_semantics.rs` asserting no un-named removal occurs.
- **Alternatives considered**: A second normalizer tuned for the new channels — rejected outright (Principle VIII violation; two normalizers = guaranteed drift).

## D7. Observers — one module per channel behind a small trait

- **Decision**: `parity-harness/src/observe/` has one module per channel (`cli_process`, `filesystem`, `image`, `container_graph`, `injected_process`, `temporal`), each implementing a `ChannelObserver` contract (`capture(ctx) -> RawChannelEvidence`). The runner invokes only the observers a case's `expected`/`fsAllowlist` declares. Filesystem capture is allowlist-scoped (Q1), never a full-tree diff.
- **Rationale**: Modular boundaries (Principle V); a case pays only for the channels it asserts; failure phase is captured from deacon's existing lifecycle vocabulary (Q5) rather than an ad-hoc string.
- **Alternatives considered**: One monolithic capture function — rejected: unreviewable, and forces every case to pay for Docker inspection even for a pure `read-configuration` CLI-process case.

## D8. Oracle-type dispatch — four explicit strategies

- **Decision**: `oracle_type.rs` dispatches on the case's `oracleType`: **spec-expectation** (compare normalized observables to declared `expected`, no reference run), **snapshot** (compare to committed provenance-checked snapshot), **live-differential** (run deacon + pinned oracle, compare normalized), **invariant/metamorphic** (evaluate a declared relationship across ≥2 operations — idempotence, first-create vs restart, resume — not a fixed output). Re-pointing a case at a different type changes only the `oracleType` field (FR-007).
- **Rationale**: The four are semantically distinct verdicts (spec FR-006/US6); conflating them weakens conformance claims. Live-differential reuses `oracle.rs` + `exec.rs`; snapshot reuses D4/D5.
- **Alternatives considered**: Treat snapshot as "differential vs a cached reference" — rejected: snapshot carries provenance/staleness semantics a live diff does not.

## D9. Allowed differences — scoped records, reuse the registry waiver model

- **Decision**: An `AllowedDifference` is `{ behavior, context, observablePath, rationale, waiverId | divergenceId }`. It applies **only** to its `(behavior, observablePath)`; the same difference elsewhere still fails (FR-033). Load-time validation rejects conflicting duplicates (FR-035) and rejects any construct that would function as a global ignore list (FR-032). `waiverId`/`divergenceId` must resolve to an existing `conformance/registry/waivers/` `wvr-*` or an `ext-`/intentional-divergence record — no parallel mechanism (FR-043).
- **Rationale**: Reuses the self-invalidating waiver model (a `wvr-` whose difference stops reproducing already fails as stale in `waiver.rs`), so FR-034 falls out of existing machinery. Keeps every tolerance auditable and tied to a characterized behavior.
- **Alternatives considered**: A per-case `ignore: [fields]` list — rejected: this *is* the global ignore list the spec forbids.

## D10. Test selection & profiles — hermetic in dev-fast, live in parity, Docker in docker groups

- **Decision**: Hermetic tests (`case_schema_valid`, `snapshot_staleness`, `allowed_difference_scoping`, `normalization_semantics`, `runner_record_replay`) run in `default`/`dev-fast`. The single live binary `parity_conformance_runner` runs **only** under `--profile parity` and is added to `fixtures/parity-corpus/registry.json` + overrides in **all** nextest profiles (parity selection + exclusions), or `parity_registry_check` fails. Docker-backed channel capture uses `docker-shared` where names are unique, `docker-exclusive` only where state is shared. Missing oracle/Docker → fail-loud `HarnessError` (no `#[ignore]`, no skip).
- **Rationale**: Matches the harness's truthful-non-selection model — a green `dev-fast` never implies live/Docker ran. Constitution VII nextest requirements + the CLAUDE.md "3-spot" override rule for new docker-exclusive binaries.
- **Alternatives considered**: An env-var opt-in (`DEACON_PARITY=1` style) — rejected: that gate was retired in 018; selection is profile-based only.

---

## Resolved unknowns summary

| Unknown | Resolution |
|---------|-----------|
| Where does the runner live? | Two existing crates, split hermetic/live (D1) |
| Case data format & location | Extend `cases.json`; coexist with legacy (D2) |
| What triggers staleness | `caseHash` over behavior-affecting inputs only, `sha2` (D3) |
| Snapshot storage | Committed `conformance/snapshots/<os>-<arch>/…`, atomic, raw+normalized separate (D4) |
| Cross-platform replay | os-arch keyed; missing → "no reference for platform" verdict (D5) |
| Normalization approach | Extend the single `normalize.rs` with named rules; reclassify existing noise lists (D6) |
| Channel capture | One observer module per channel; allowlist-scoped filesystem (D7) |
| Oracle types | Four explicit dispatch strategies (D8) |
| Tolerated differences | Scoped `AllowedDifference` reusing the waiver model (D9) |
| Test selection | Hermetic dev-fast + one live parity binary + docker groups (D10) |

No `NEEDS CLARIFICATION` remain. Proceed to Phase 1.
