# Phase 1 Data Model: Declarative Conformance Runner

**Feature**: 022-conformance-runner | **Date**: 2026-07-24

All records are strict-JSON, version-controlled, hand-editable (snapshots are machine-written by the reviewed refresh, then committed). Field names are `camelCase` to match the existing registry. "Existing" marks fields already present in `cases.json`/`channels.json`.

---

## 1. Case (extends `conformance/registry/cases.json` record)

The declarative unit. A record is **legacy** (has `executable.binary`) or **declarative** (has `operations`); never both.

| Field | Type | Req | Notes |
|-------|------|-----|-------|
| `id` | string | ✓ | *existing*; `case-…`, globally unique (V2) |
| `behaviors` | string[] | ✓ | *existing*; ≥1 `bhv-…` in registry (V3) |
| `context` | string[] | ✓ | *existing*; may be empty; V10 intersection with behavior context |
| `oracleType` | enum | ✓ (declarative) | `spec-expectation` \| `snapshot` \| `live-differential` \| `invariant-metamorphic` |
| `operations` | Operation[] | ✓ (declarative) | ordered; ≥1; the actions the runner performs |
| `expected` | ExpectedObservable[] | ✓ (declarative) | per-channel expectations; each `channel` ∈ `channels.json` (V9) |
| `allowedDifferences` | AllowedDifference[] | — | scoped tolerances; default `[]` |
| `fsAllowlist` | string[] | — | path/glob allowlist for the filesystem channel (rooted at workspace or declared out-dir); required iff a `chan-filesystem`/`chan-file-content` expectation exists |
| `cleanup` | Cleanup | ✓ (declarative) | resources to reclaim after run (success or failure) |
| `resourceGroup` | enum | — | nextest group for Docker cases: `docker-shared` \| `docker-exclusive` \| `fs-heavy` \| `none` (default `none`) |
| `notes` | string | — | prose; **excluded from `caseHash`** |
| `executable` | object | ✓ (legacy) | *existing* `{ binary, corpus?, case? }`; legacy path only |
| `outcomes` | object[] | ✓ (legacy) | *existing* `{ channel, expectation }`; legacy path only |

**Validation** (new V-series classes, continuing after V14):
- A record MUST be exactly one of legacy/declarative (both or neither → violation).
- `oracleType: invariant-metamorphic` MUST declare ≥2 operations and a `relationship` (see Operation) — arity check.
- Every `expected.channel` and every `allowedDifferences[].observablePath` root MUST reference a declared channel.
- `fsAllowlist` required-iff a filesystem-channel expectation exists (and forbidden otherwise, to keep capture scoped).

**caseHash**: SHA-256 over canonical JSON of `{operations, oracleType, expected, fsAllowlist, referenced fixtureHashes}` — **not** `notes`/`allowedDifferences`/`behaviors` prose. (research D3)

---

## 2. Operation

A single action the runner performs. Ordered within a case.

| Field | Type | Req | Notes |
|-------|------|-----|-------|
| `id` | string | ✓ | unique within the case (referenced by metamorphic `relationship`) |
| `subcommand` | enum | ✓ | consumer surface only: `up`\|`down`\|`exec`\|`build`\|`read-configuration`\|`run-user-commands`\|`templates-apply`\|`doctor` (Principle II) |
| `argv` | string[] | ✓ | args after the subcommand; part of `caseHash` |
| `fixtures` | string[] | — | fixture ids this op materializes into the workspace |
| `stdin` | string | — | optional stdin payload |
| `expectFailurePhase` | enum | — | for negative cases: one of the closed failure-phase set (§8) |
| `relationship` | Relationship | — | invariant/metamorphic only: `{ kind: idempotence\|first-create-vs-restart\|resume, againstOp: <op id> }` |

---

## 3. Fixture

| Field | Type | Req | Notes |
|-------|------|-----|-------|
| `id` | string | ✓ | referenced by `Operation.fixtures` |
| `path` | string | ✓ | repo-relative source (pinned inputs; images pinned by digest/tag, no `latest`) |
| `fixtureHash` | string | ✓ (derived) | SHA-256 of the fixture bytes; feeds `caseHash` and provenance |

---

## 4. Channel (extends `conformance/registry/channels.json`)

*Existing*: `chan-exit-code`, `chan-stdout`, `chan-stderr`, `chan-filesystem`, `chan-file-content`, `chan-container-state`.
**New** (this feature):

| id | Covers |
|----|--------|
| `chan-structured-output` | Parsed structured (JSON) result document distinct from raw `chan-stdout` |
| `chan-image` | Built-image configuration + metadata (labels parsed semantically) |
| `chan-process-graph` | Container + network + volume + mount graph |
| `chan-injected-process` | Env, user, cwd, PATH resolution, signals, TTY, exit propagation |
| `chan-temporal` | Lifecycle ordering, first-create vs restart, resume, cleanup transitions |

(`chan-container-state` is retained for legacy cases; new cases use the finer-grained `chan-process-graph`/`chan-image`/`chan-injected-process`.)

---

## 5. ExpectedObservable

| Field | Type | Req | Notes |
|-------|------|-----|-------|
| `channel` | string | ✓ | a declared channel |
| `operation` | string | — | which op produced it (default: last) |
| `assertion` | object | ✓ | channel-specific expectation shape (see contracts/observer-channel.md) |

For `oracleType: live-differential`/`snapshot`, `assertion` MAY be omitted for a channel (the reference/snapshot supplies the expectation); for `spec-expectation` it is required.

---

## 6. AllowedDifference (research D9)

| Field | Type | Req | Notes |
|-------|------|-----|-------|
| `behavior` | string | ✓ | a `bhv-…` linked by the case |
| `context` | string[] | ✓ | context tags the tolerance applies under |
| `observablePath` | string | ✓ | dotted path within a channel (e.g. `chan-injected-process.env.TZ`); NOT a whole channel unless the behavior is channel-wide |
| `rationale` | string | ✓ | why this difference is acceptable |
| `waiverId` \| `divergenceId` | string | ✓ (one) | resolves to `conformance/registry/waivers/wvr-*` or an `ext-`/intentional-divergence record |

**Rules**: applies only to `(behavior, observablePath)` (FR-033); duplicate `(behavior, observablePath)` with differing definitions → load error (FR-035); any construct matching a whole channel with no path and no behavior-wide justification → rejected as a global ignore list (FR-032).

---

## 7. Snapshot & Provenance (committed under `conformance/snapshots/<os>-<arch>/<case-id>/`)

Three files, raw and normalized **separate** (FR-016).

### `provenance.json` — the 13 required elements (FR-017)

| Field | Type | Notes |
|-------|------|-------|
| `oracleVersion` | string | pinned `@devcontainers/cli` version (verified via `oracle.rs`) |
| `sourceRevision` | string | pinned spec/source revision (e.g. `113500f4`) |
| `caseHash` | string | §1 |
| `fixtureHash` | string | combined fixture hash |
| `argv` | string[] | full argv actually executed |
| `platform` | string | OS (e.g. `linux`, `darwin`) |
| `arch` | string | architecture (e.g. `x86_64`, `aarch64`) |
| `nodeVersion` | string | Node used by the oracle |
| `dockerVersion` | string | Docker engine version |
| `composeVersion` | string | Compose version |
| `imageDigests` | map<string,string> | image ref → digest for every pinned image used |
| `normalizerVersion` | string | `NORMALIZER_VERSION` constant (research D6) |
| `capturedAt` | string | provenance timestamp (informational; NOT one of FR-017's thirteen; not part of staleness) |

The twelve rows above `capturedAt` are the FR-017 identity/environment elements stored in `provenance.json`. `capturedAt` is an informational thirteenth *provenance-file* field — **not** one of FR-017's thirteen and **excluded** from staleness. The final FR-017 element — the captured observables — is stored in the sibling `raw.json`/`normalized.json` (FR-016), not in `provenance.json`. `argv` is the complete argument vector with temporary paths tokenized (`<WORKSPACE>`) for portability; `raw.json` retains the verbatim argv.

`raw.json`: verbatim per-channel captured evidence. `normalized.json`: rule-normalized evidence.

### Staleness (FR-020, SC-003)

Replay recomputes the evidence-determining inputs. Replay **fails as stale**, naming the first mismatch, when any of: `caseHash`, `fixtureHash`, `oracleVersion`, `sourceRevision`, `imageDigests`, `normalizerVersion` differs. `platform`/`arch` mismatch is **not** staleness — it selects a different snapshot; absence of a snapshot for the current `os-arch` → **"no reference for platform"** verdict (FR-016a). The host tool versions `nodeVersion`/`dockerVersion`/`composeVersion` are recorded for reproducibility but are **not** staleness signals — gating on them would make a snapshot stale on every machine but the recorder's, defeating cross-machine CI replay (SC-003); a genuine host-version effect on evidence surfaces as a divergence on replay instead.

---

## 8. Failure Phase (closed set — clarify Q5)

Reuses deacon's lifecycle/execution vocabulary; not an open string:
`config-resolution` → `build` → `container-create` → `lifecycle:onCreate` → `lifecycle:updateContent` → `lifecycle:postCreate` → `lifecycle:postStart` → `lifecycle:postAttach` → `exec`.

---

## 9. Evidence & Verdict (runtime, not committed except via snapshot)

- **RawChannelEvidence**: `{ channel, operation, present: bool, value: Value }` — `present:false` distinguishes not-captured from captured-but-empty (FR-018).
- **NormalizedChannelEvidence**: same shape after named rules; `null`/empty/default kept distinct (FR-025).
- **ChannelVerdict**: `{ channel, outcome: agree|diverge|allowed-difference|no-reference-for-platform|stale|error, detail }` (FR-015).
- **CaseVerdict**: `{ caseId, oracleType, channels: ChannelVerdict[], overall }` — `overall = worst channel outcome`. Attributable to the linked behavior(s) (FR-042).

---

## Entity relationships

```text
Case 1─* Operation *─* Fixture
Case *─* Behavior (registry)        Case 1─* ExpectedObservable ─1 Channel
Case 1─* AllowedDifference ─1 (Behavior, observablePath) ─1 Waiver|Divergence (registry)
Case 1─* Snapshot (per os-arch) ─1 Provenance ─ {raw.json, normalized.json}
Runner: Case × Target(deacon|reference|snapshot) → Evidence(raw,normalized) → CaseVerdict(ChannelVerdict[])
```

## State transitions (a run)

```text
load case ──(malformed/unknown behavior)──▶ FAIL-LOUD (FR-003)
   │ ok
materialize fixtures in isolated external workspace (Docker cases)
   │
for each operation: execute against target ──(missing oracle/Docker)──▶ FAIL-LOUD (no skip)
   │  (crash mid-op → record partial evidence + failurePhase)
capture declared channels → raw evidence
   │
normalize (named rules) → normalized evidence (raw + normalized persisted separately)
   │
compare per oracleType, applying scoped allowed differences
   │
emit CaseVerdict ──▶ cleanup (RAII, runs on success AND failure)
```
