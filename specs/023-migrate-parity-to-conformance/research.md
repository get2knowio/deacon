# Phase 0 Research: Migrate Parity Assets into the Declarative Conformance System

**Branch**: `023-migrate-parity-to-conformance` | **Date**: 2026-07-24
**Input**: [spec.md](./spec.md) — FR-001–FR-005 require the exact baseline to be established here, not recalled.

---

## 1. The Enumerated Baseline (FR-001 – FR-005)

Enumerated from the repository at the branch point (`98c26a5`), applying the FR-049 rule: **a baseline unit is the finest granularity for which the current system reports an independent outcome** — each per-case result a comparison program emits — **plus each test function that emits no per-case result**, counting as one unit.

### 1a. Live comparison programs (run only under `--profile parity`)

| Program | Units | Case ids emitted |
|---|---:|---|
| `parity_read_configuration` | 2 | `basic`, `with-variables` |
| `parity_exec` | 4 | `working-directory`, `user`, `tty`, `env-propagation` |
| `parity_build` | 6 | `creates-discoverable-image`, `with-build-args`, `push-json-output`, `output-json-format`, `buildkit-only-features`, `image-reference` |
| `parity_up_exec` | 1 | `traditional` |
| `parity_observable_state` | 7 | `lockfile-manifest-digest`, `compose-config-mounts`, `compose-project-name-isolated`, `container-and-image-labels`, `rendered-compose-state`, `handoff-no-reuse`, `merged-config-vs-runtime` |
| `parity_state_diff` | 8 | `single-container-parity`, `compose-parity-with-feature-mount-gap`, `intra-deacon-single-vs-compose`, `default-workspace-mount-target-parity`, `dockerfile-build-and-nonroot-user`, `appport-published-ports`, `mount-variety-readonly-and-tmpfs`, `compose-sidecar-and-named-volume` |
| `parity_corpus_tier1` | 24 | discovered (see 1d) |
| `parity_corpus_merged` | 24 | same 24 dirs, `--include-merged-configuration` mode |
| `parity_corpus_errors` | 9 | discovered (see 1d) |
| `parity_conformance_runner` | 6 | the declarative cases it drives |
| **Subtotal** | **91** | |

### 1b. Hermetic guard programs (emit no per-case result → one unit per test function)

| Program | Units | Role |
|---|---:|---|
| `parity_harness_faults` | 10 | fault injection `a`–`j`: version mismatch, missing override, docker-missing, oracle crash, malformed output, timeout w/ partial output, injected divergence, matching waiver, stale waiver, normalization failure with no raw fallback |
| `parity_registry_check` | 6 | registry ↔ tests ↔ nextest agreement, corpus minimums, no-`#[ignore]` idiom scan, waiver location, anchor validity |
| **Subtotal** | **16** | |

### 1c. Internal-consistency programs (registered in `registry.json`)

| Program | Units |
|---|---:|
| `consistency_env_probe_flag` | 2 |
| `consistency_remote_env_flags` | 2 |
| **Subtotal** | **4** |

### 1d. Corpora and fixtures

| Fixture group | Count | Location |
|---|---:|---|
| Tier-1 valid corpus case dirs | **24** | `fixtures/parity-corpus/*/` with `.devcontainer/` |
| Error corpus case dirs | 9 | `fixtures/parity-corpus/errors/*/` |
| Config fixtures used by `parity_read_configuration` | 2 | `fixtures/config/{basic,with-variables}/` |
| Declarative fixtures | 2 | `conformance/fixtures/{fx-config-with-unknown-field,fx-up-basic}/` |
| **In-repo fixture dirs** | **37** | |
| Inline (code-authored) fixtures in scenario programs | 8 in `parity_state_diff` + inline writes in `parity_exec` / `parity_build` / `parity_up_exec` | written to temp dirs at runtime |
| Pinned external real-world corpus entries | 33 | `fetch_realworld_corpus.py` (fetch-only, never run in CI) |

### 1e. Normalization rules

| Family | Rules |
|---|---|
| Channel-scoped named rules (022) | `path_token`, `null_preserving`, `label_semantic`, `mount_source_canonical`, `path_env_segmented`, plus composites `normalize_image` / `normalize_process_graph` / `normalize_injected_process` and the `normalize_channel` dispatcher |
| Legacy config-scoped | `config` (unwraps the reference's `{configuration}` envelope), `merged_config` (extracts `mergedConfiguration`), `prune` (**blanket**: drops every null / empty object / empty array / empty string value, plus `DROP_KEYS = ["configFilePath"]`), `sanitize_dynamic_values` (`${devcontainerId}` → `<ID>`, and `replace_hex12`: any 12-char lowercase-hex run → `<ID>`) |
| Legacy state-scoped | `container_state`, `diff_states`, `drop_noise_env` (`is_noise_env_key`), `strip_intentional_labels` (`is_intentional_label`, prefix match) |

### 1f. Difference and result vocabularies

| Vocabulary | Members |
|---|---|
| `normalize::DiffKind` | `RefOnly`, `Value`, `DeaconOnly` (ranked 0/1/2) |
| `report::Outcome` | `Pass`, `PassWaived`, `Fail` |
| `report::Cause` | `Divergence`, `OracleFailure`, `OracleTimeout`, `MalformedOutput`, `Normalization`, `FixtureMissing`, `DockerMissing` |
| `evidence::Outcome` (declarative) | `Agree`, `Diverge`, `AllowedDifference`, `NoReferenceForPlatform`, `Stale`, `Error` |
| corpus `ProcessOutcome` | `BothSucceeded`, `Waived`, `Failed` |
| `waiver::Expect` | `ReferenceStricter`, `DeaconStricter`, `BothReject`, `BothAccept`, `FieldDivergence` |

### 1g. Registry state

| Record | Count |
|---|---:|
| Cases | 31 — **25 legacy pointers**, 6 declarative |
| Behaviors | 25 (all covered; zero uncovered) |
| Channels | 11 |
| Waivers (`wvr-`) | 10 |
| Extensions (`ext-`) | 6 |
| Gaps | 0 |
| Profiles / dimensions | 1 active profile / 4 dimensions |
| Committed snapshots | 1 (`linux-x86_64/case-readconfig-snapshot`) |

### Baseline totals

| Category | Units |
|---|---:|
| Live comparison per-case units | 91 |
| Hermetic guard units | 16 |
| Internal-consistency units | 4 |
| **Total baseline units** | **111** |
| Characterized exceptions (10 `wvr-` + 6 `ext-`) | 16 |
| Fixture dirs (in-repo) | 37 |
| Normalization rules | 19 |

---

## 2. Decisions

### D1 — The recalled corpus count was wrong; enumeration is the only acceptable source

**Decision**: The Tier-1 valid corpus contains **24** cases, not 25.

**Rationale**: `ls -d fixtures/parity-corpus/*/` yields 25 entries, but one is `errors/`, which `discover_tier1_cases` excludes along with dot-dirs, `waivers`, and `__pycache__`; discovery additionally requires a `.devcontainer/` subdirectory. The `/speckit.specify` survey reported 25 by counting directories rather than by applying the discovery rule. This is precisely the failure mode FR-001 exists to prevent, discovered on the first attempt to enumerate.

**Consequence**: The baseline generator MUST reuse the *production discovery functions* (`discover_tier1_cases`, `discover_error_cases`) rather than re-implementing directory walks, so the baseline cannot drift from what the runners actually execute.

**Alternatives considered**: Hand-maintained case list in the baseline file — rejected: it would need the same drift gate and would encode the bug again.

---

### D2 — The registry under-counts real coverage by ~3.6×; the pointer cases are the defect

**Decision**: Treat the 25 legacy pointer cases as *placeholders standing in for* 111 baseline units, and make one-to-one mapping the core migration work.

**Rationale**: Mapping pointer cases to enumerated units exposes the scale of the loss:

| Program | Pointer cases | Baseline units | Under-count |
|---|---:|---:|---:|
| `parity_corpus_tier1` | 1 | 24 | −23 |
| `parity_corpus_merged` | 2 | 24 | −22 |
| `parity_state_diff` | 1 | 8 | −7 |
| `parity_observable_state` | 2 | 7 | −5 |
| `parity_build` | 1 | 6 | −5 |
| `parity_exec` | 1 | 4 | −3 |
| `parity_read_configuration` | 1 | 2 | −1 |
| `parity_corpus_errors` | 9 | 9 | 0 ✓ |
| `parity_up_exec` | 2 | 1 | +1 |
| `parity_conformance_runner` | — | 6 | (already declarative) |

Only the error corpus is honestly represented today. `parity_up_exec` is the inverse defect: two behaviors are claimed from a single reported outcome, so one of them has no independent evidence.

**Consequence**: Post-migration case count is expected to *rise* from 31 to ≈111 while the behavior count stays at 25 or falls — exactly the SC-005 shape. A rising case count here is the migration working, not scope creep.

**Alternatives considered**: Keeping coarse pointer cases and accepting the under-count — rejected: it makes the registry's coverage claim false, which is the whole reason the record exists.

---

### D3 — `prune` is the deacon-only-as-noise assumption, and it is the migration's central normalization defect

**Decision**: The legacy config normalizer's `prune` MUST NOT be carried into the declarative path. Config comparison migrates onto `null_preserving` semantics, and every field `prune` currently removes must become either a compared value, a named field-scoped rule, or a characterized exception.

**Rationale**: The 022 channel path already satisfies FR-021 — every rule is named, scoped, and rewrites rather than deletes, and `null_preserving` explicitly documents that nothing is pruned. The legacy config path does the opposite: `prune` recursively drops **every** null, empty object, empty array, and empty string value anywhere in the document, plus `configFilePath` unconditionally. `DiffKind::DeaconOnly` is then ranked *lowest* with the comment "usually default noise". Together these mean a deacon-only field that happens to be empty is invisible, and a deacon-only field that is populated is reported at the lowest priority. That is the assumption FR-020 forbids in as many words.

`sanitize_dynamic_values` carries a second, narrower risk: `replace_hex12` rewrites **any** 12-character lowercase-hex run to `<ID>`, which will silently collapse legitimate content (short digests, hashes, hex-looking identifiers). It is applied identically to both sides so it cannot manufacture a false pass on differing values, but it can mask a real difference between two distinct hex values. It becomes a named, field-scoped rule or it goes.

**Consequence**: Migrating the 48 corpus units (24 tier-1 + 24 merged) is expected to *surface new differences* that `prune` was hiding. Per FR-036 this is a strictness improvement and is permitted; each newly surfaced difference is characterized as it appears. This is the single largest source of unplanned work in the feature and should be scheduled early, not late.

**Alternatives considered**:
- Port `prune` verbatim as a named rule "drop_empty_values" — rejected: naming a blanket rule does not make it scoped; FR-021 forbids removing a *category*.
- Keep `prune` only for the merged-config channel — rejected: the same fidelity argument applies to both modes, and it would preserve two comparison rule paths, violating FR-030.

---

### D4 — Deletion order is dictated by the residual set, so residuals are sized before any deletion

**Decision**: Sequence the migration by *carrier*, and determine each carrier's residual set before touching it. Predicted residual pressure, highest first:

| Carrier | Predicted residual risk | Missing capability |
|---|---|---|
| `parity_state_diff` (8) | **High** | cross-CLI state snapshot comparison; intra-deacon single-vs-compose (two deacon runs compared to each other, no reference side) |
| `parity_observable_state` (7) | **High** | reference-container handoff/no-reuse; rendered compose state; runtime-truth-vs-merged-config |
| `parity_build` (6) | Medium | image discovery by label; push/registry interaction |
| `parity_exec` (4), `parity_up_exec` (1) | Low–medium | already close to the declarative operation model |
| `parity_corpus_tier1` / `_merged` (48) | **Low** | pure `read-configuration` differential — the declarative model already covers this shape |
| `parity_corpus_errors` (9) | **Low** | already 1:1, waivers already migrated |
| `parity_read_configuration` (2) | **Low** | same shape as the corpus runners |

**Rationale**: A residual blocks deletion of its carrier (FR-013), so a carrier is deletable only when *all* of its units migrate. The 83 low-risk units (corpora + read-configuration + errors) can migrate and let three whole programs be deleted; the 15 high-risk units concentrate in two programs that will likely survive this feature carrying residuals.

**Consequence**: Realistic outcome — `parity_corpus_tier1`, `parity_corpus_merged`, `parity_corpus_errors`, and `parity_read_configuration` are deleted; `parity_state_diff` and `parity_observable_state` very likely persist with residual records and tracked follow-ups. This is the honest expectation and should be stated in the PR, not discovered at the end.

**Alternatives considered**: Migrating high-risk carriers first to de-risk — rejected: it front-loads runner extension work (out of scope per the spec) before any conservation machinery exists to prove the low-risk migrations were lossless.

---

### D5 — Two comparison paths must coexist transitionally, which is a knowing Principle VIII exception

**Decision**: During migration, the legacy config/state normalizers and the declarative channel normalizer both exist. This is time-boxed by the US7 equivalence gate and recorded in Complexity Tracking.

**Rationale**: FR-030 and Constitution VIII both forbid a second implementation of a comparison rule. But the equivalence gate (FR-033) *requires running both paths over the full baseline* to prove the replacement is not more permissive — which is impossible if the old path is deleted first. The exception is inherent to any proven-safe migration.

**Consequence**: The coexistence window is bounded by an explicit invariant: no new case may be added to the legacy path after the migration begins, and the legacy path is deleted the moment its last unit's equivalence is proven. A hermetic test enforces the first half (legacy carrier case counts may only decrease).

**Alternatives considered**: Cut over without proof — rejected outright; it is the exact failure the feature exists to prevent.

---

### D6 — Baseline and coverage machinery live in `deacon-conformance` (hermetic); equivalence lives in `parity-harness` (live)

**Decision**: Split on the existing 022 seam. Hermetic data/validation/accounting → `deacon-conformance`. Live execution/observation/comparison → `parity-harness`.

**Rationale**: This is the split 022 already established and that `certify`/`validate`/`snapshot check` already follow. Baseline enumeration reads repository files and registry data — no Docker, no network, no oracle — so it belongs in the lower crate and can gate every PR (FR-052, SC-017). The equivalence ledger needs both paths executed against the live oracle, so it belongs in the harness and runs only in the parity lane.

**Consequence**: New hermetic commands extend the existing `deacon-conformance` bin; the equivalence comparison is a new `parity-harness` bin alongside `parity-report` and `conformance-snapshot`. No new crate.

**Alternatives considered**: A separate `migration-harness` crate — rejected: a third crate would need its own registry loaders, re-creating the duplication the feature is removing.

---

### D7 — Baseline unit identity is derived; case identity is authored

**Decision**: A baseline unit's identity is `<program>::<case-id>` (or `<program>::<test-fn>` for guard programs) — mechanically derived and therefore un-gameable. The migrated case's identity is a hand-authored `case-*` slug (FR-050). The mapping between them is explicit data.

**Rationale**: Deriving baseline identity means the drift check can recompute it; authoring case identity keeps registry ids readable and stable under content edits, matching existing convention and 022's `caseHash`-for-staleness split. An explicit mapping table is what makes the coverage report a *proof* rather than a count comparison — two sets of equal size can still have lost an item.

**Alternatives considered**: Content-hash case ids (021 clause style) — rejected: clause ids are substance-anchored because clauses are extracted from prose we do not author; cases are authored artifacts, and a hash id would churn on every edit and break committed snapshot provenance.

---

### D8 — The external real-world corpus is recorded, not executed

**Decision**: The 33 manifest entries become baseline items of category `external-corpus-entry` with identity `realworld::<name>`, mapped to a residual record naming "no vendored fixture / network fetch required". `fetch_realworld_corpus.py` survives the migration as a fetch utility.

**Rationale**: Per the spec clarification, it is a coverage *source*, not a comparison program — it has never run in CI and asserts nothing today. Recording it preserves the knowledge that these workspaces were selected as representative; vendoring 33 third-party workspaces is out of scope and would violate the "no vendored third-party content" intent of the original script.

**Consequence**: These 33 items count toward the baseline's recorded inventory but never toward "migrated" and never block certification. They are the one baseline category expected to remain residual indefinitely.

**Alternatives considered**: Excluding them from the baseline entirely — rejected: FR-002 names the manifest explicitly, and silent exclusion is the loss mode the feature prevents.

---

### D9 — Failure-class verification extends the existing fault-injection binary

**Decision**: Extend `parity_harness_faults`'s stub-executable approach to the declarative runner rather than adding a new mechanism, adding one hermetic case per difference class and per process-level cause.

**Rationale**: `parity_harness_faults` already proves nine of the needed classes hermetically using stub executables (wrong version, nonexistent binary, failing docker probe, crash, garbage output, hang) and synthetic evidence (injected difference, matching waiver, stale waiver, normalization failure). It runs in every lane with no Docker and no oracle. FR-056 and FR-048 are already satisfied by this pattern; a second mechanism would violate FR-030.

**Gap to close**: The baseline's 10 fault tests cover `report::Cause` and waiver behavior but not the declarative `evidence::Outcome` vocabulary (`AllowedDifference`, `NoReferenceForPlatform`, `Stale`) nor `DiffKind::DeaconOnly` specifically. Migration adds those cases.

**Alternatives considered**: Live fault injection against a real oracle — rejected: it would move the guarantee into the parity lane, which does not gate ordinary PRs.

---

## 3. Resolved Unknowns

| Unknown from Technical Context | Resolution |
|---|---|
| Exact baseline composition | §1 — 111 units, enumerated (D1) |
| What counts as one unit | FR-049; derived identity per D7 |
| Whether new crates are needed | No — D6 splits across the two existing dev-only crates |
| Whether new runtime dependencies are needed | No — `serde`/`serde_json`, `indexmap`, `sha2`, `tokio`, `thiserror`, `tracing`, `toml`, `tempfile` are all present |
| How deacon-only data is currently handled | `prune` + lowest-ranked `DiffKind::DeaconOnly` — the defect D3 fixes |
| Which carriers can actually be deleted | D4 — the 83 low-risk units; the 15 high-risk ones likely persist as residuals |
| Where migration machinery lives | D6 |
| How failure classes are proven | D9 |

**No `NEEDS CLARIFICATION` markers remain.**

---

## 4. Deferred Work (Constitution I — Deferral Tracking)

These MUST appear in `tasks.md` under `## Deferred Work` with acceptance criteria:

- **[Deferral, D4]** Migrate `parity_state_diff`'s 8 units — requires a declarative cross-CLI state-snapshot capability and an intra-deacon (reference-free) comparison mode. Acceptance: all 8 units declarative, `parity_state_diff` deleted, equivalence ledger clean.
- **[Deferral, D4]** Migrate `parity_observable_state`'s 7 units — requires container-handoff and rendered-compose observation. Acceptance: all 7 units declarative, program deleted.
- **[Deferral, D8]** Decide the long-term disposition of the 33 external real-world corpus entries (vendor, prune the manifest, or retain as permanent residual). Acceptance: an explicit registry disposition, not an open residual queue entry.
- **[Deferral, D3]** Characterize every difference newly surfaced by removing `prune`. Acceptance: zero uncharacterized differences in the corpus units; each new difference is a case, waiver, or tracked fix issue.
