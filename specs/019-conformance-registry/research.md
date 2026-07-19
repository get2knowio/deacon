# Research: Repository-Owned Conformance Registry

**Feature**: `019-conformance-registry` | **Date**: 2026-07-19

All spec-level unknowns were resolved during `/speckit.clarify`. This document records the
implementation-shaping decisions and their rationale.

## Decision 1: Registry data lives at `conformance/registry/`, code in a new dev-only crate `crates/conformance/`

**Decision**: Registry data files are JSON under a new top-level `conformance/registry/`
directory. Validation, reporting, and certification logic live in a new workspace crate
`crates/conformance/` (package `deacon-conformance`, `publish = false`, like
`parity-harness`), exposing a library plus a single `conformance` binary with
`validate` / `report` / `certify` subcommands.

**Rationale**:
- The registry is authoritative project data, not test-support material — `fixtures/`
  connotes test inputs. A top-level `conformance/` directory signals authority (invalid
  registry *fixtures* for acceptance tests DO go under `fixtures/conformance/`).
- A separate crate honors constitution V (modular boundaries) and lets `parity-harness`
  depend on the registry record types without inverting dependencies. It stays out of the
  published `deacon`/`deacon-core` surface (constitution II — this is contributor tooling,
  not a consumer CLI command).
- One binary with subcommands (vs three binaries) minimizes nextest/CI surface.

**Alternatives considered**:
- *Extend `parity-harness`*: rejected — the registry's scope (schema constraints, spec
  clauses, CLI surface) is a superset of parity concerns; parity-harness would become a
  monolith, and the dependency direction (harness consumes registry) would be muddled.
- *Data under `fixtures/parity-corpus/`*: rejected — wrong connotation and wrong scope;
  the registry outlives and supersedes parts of the parity corpus.

## Decision 2: JSON as the registry format, one file per collection, one file per waiver

**Decision**: Registry records are JSON: single-file collections for small closed sets
(`revisions.json`, `dimensions.json`, `channels.json`, `profiles.json`, `gaps.json`,
`extensions.json`, `cases.json`), per-inventory files for source units
(`sources/{schema,spec,cli,observed}.json`), per-area files for behaviors
(`behaviors/<area>.json`), and one file per waiver (`waivers/<id>.json`, preserving the
existing parity-waiver granularity so diffs stay reviewable).

**Rationale**: `serde_json` + `indexmap` are already workspace deps; JSON matches the
parity corpus, keeps parsing dependency-free, and per-waiver files preserve today's
review ergonomics. Declaration order is preserved with `IndexMap`/`Vec` (constitution VI
ordering rule).

**Alternatives considered**: TOML (worse for nested arrays-of-objects, new parse dep in
this crate), YAML (new dep, whitespace-fragile), a single monolithic registry.json
(merge-conflict magnet — the same reason `nextest.toml` filter lines conflict today).

## Decision 3: Waiver records migrate INTO the registry; parity-harness loads them from it

**Decision**: The record schema and loaders for waivers move to `deacon-conformance`.
`fixtures/parity-corpus/waivers/*.json` and `fixtures/parity-corpus/errors/*/expect.json`
are migrated to `conformance/registry/waivers/wvr-*.json` (schema extended with registry
fields: stable `wvr-` ID, behavior links, expiry). The legacy files are deleted;
`parity-harness::waiver::WaiverSet` becomes a thin query wrapper over records loaded from
the registry (its public query API — `corpus_case`, `state_field_waivers`, `stale_among` —
is preserved so the nine live parity binaries and `parity_corpus_errors` need only
path/type-level changes, not logic changes).

**Rationale**: Clarification Q3 chose migration-with-unchanged-execution. Keeping the
query API stable confines churn to `waiver.rs` internals and the data files. The
self-invalidating-waiver mechanic (stale waiver fails the run) is preserved — it becomes
the registry's reference-status evidence loop.

**Alternatives considered**: pointer-only `expect.json` files that name a registry ID
(rejected: two files per fact, the duplication FR-027 exists to kill); leaving the error
corpus alone (rejected: violates single-source-of-truth, SC-001).

## Decision 4: Waiver expiry and "today"

**Decision**: Expiry is an ISO-8601 calendar date (`"expires": "YYYY-MM-DD"`), compared
lexicographically against today's UTC date (valid through the stated date, per spec
Assumptions). Today's date is obtained via the `jiff` crate (new dev-only dependency of
`deacon-conformance` only) and is injectable (`validate --today YYYY-MM-DD`, and a
parameter at the library layer) so acceptance tests are deterministic, including the
boundary case (expiry == today passes; expiry < today fails).

**Rationale**: ISO dates compare correctly as strings, so no date arithmetic is needed —
only "current date", which stdlib does not provide civil-date conversion for. `jiff` is
modern, actively maintained, MSRV-compatible, and confined to the dev-only crate
(constitution V dependency hygiene). Injection satisfies FR-029's deterministic-test
mandate. Seeded migrated waivers get `expires` dates set ~6 months out (2027-01-19) so
the seeded registry validates cleanly while forcing periodic re-review.

**Alternatives considered**: `time` crate (fine, but `jiff` has the cleaner civil-date
API); hand-rolled days-since-epoch conversion (needless correctness risk); no default —
always require `--today` (hostile ergonomics for the primary "maintainer runs validate"
flow).

## Decision 5: Contradiction rule set (extends FR-014's minimum)

**Decision**: The enforced disposition contradiction rules, documented in
`conformance/registry/RULES.md` and encoded in validation:

| Rule | Statement |
|------|-----------|
| R1 | decision `unresolved-gap` contradicts (spec `conformant` AND reference `aligned`) |
| R2 | decision `deacon-extension` requires spec ∈ {`unspecified`, `not-applicable`} |
| R3 | decision `intentional-divergence` contradicts reference `aligned` |
| R4 | reference `unknown` on an in-profile behavior requires decision `unresolved-gap` |
| R5 | decision `follow-spec` requires spec `conformant` |
| R6 | decision `align-with-reference` requires reference `aligned` |
| R7 | a behavior whose only structural coverage is a gap record requires decision `unresolved-gap` |
| R8 | an in-profile behavior with no test case **and no waiver** requires reference `unknown` (statuses are verified claims, not aspirations) |

R1–R4 are FR-014(a)–(d); R5–R8 close the remaining aspirational-status loopholes. The
chain R8→R4→R7 is what makes incremental population coherent: no case/waiver ⇒ reference
unknown ⇒ decision unresolved-gap ⇒ gap record required ⇒ structural validation passes,
strict certification blocks.

**Rationale**: Statuses must be *evidence-backed*: "conformant/aligned" without a case is
exactly the ambiguity the three-axis model exists to eliminate. A behavior that deacon
believes-but-hasn't-verified is, honestly, a gap. A waiver counts as evidence for a
`divergent` status because the parity harness *verifies* waivers keep reproducing (a
waiver whose difference stops reproducing fails the run as stale) — so waiver-only
coverage legitimately backs `reference: divergent` without forcing `unresolved-gap`
(keeps R8 consistent with FR-018e's case/waiver/gap coverage triad).

**Alternatives considered**: only rules (a)–(d) (leaves "declared aligned, never tested"
representable — defeats SC-005); requiring waivers to imply specific decisions (rejected:
a waiver can legitimately accompany `follow-spec` when the *reference* is wrong).

## Decision 6: Seed inventory (FR-026 enumeration)

**Decision**: The seeded registry migrates/records, as of this feature:

1. **Parity waivers** (1): `extends-child-merged` (tier1, reference-stricter).
2. **Error-corpus expectations** (9): `bad-config-path`, `duplicate-keys`,
   `extends-cycle`, `extends-missing`, `malformed-json`, `missing-config`,
   `unknown-field-preserved`, `wrong-type-features`, `wrong-type-forwardports`.
   `both-accept`/`both-reject` ones seed aligned behaviors with their cases. The
   `deacon-stricter` ones split by family: the strictness cases (`malformed-json`,
   `wrong-type-*`) seed divergent behaviors (spec `unspecified` or `conformant`,
   reference `divergent`, decision `intentional-divergence`); the **extends family**
   (`extends-cycle`, `extends-missing`, and tier1 `extends-child`) seeds behaviors with
   spec `unspecified` (in-flight proposal devcontainers/spec#22), reference `divergent`,
   decision `deacon-extension` — linked from `ext-extends-resolution` so V8's
   extension-consistency check holds; the parity waivers coexist with those behaviors.
3. **`docs/DIFFERENTIATORS.md` behavioral entries**: `--secrets-file` .env superset
   (extension), strict-on-mistakes validation posture (intentional divergence — same
   behaviors as the error corpus, deduplicated per FR-028), `extends` resolution
   (intentional divergence, ahead-of-spec, issue #297), compose project-name derivation
   robustness (intentional divergence), workspace-trust gate (extension, per SECURITY.md).
   Non-behavioral entries (single static binary, env-probe caching performance) are NOT
   behaviors — recorded nowhere, noted as out-of-scope in RULES.md.
4. **CLAUDE.md "Verified Non-Bugs" divergence entries**: read-configuration strictness
   family (dedupes into #2), compose marker cleanup parity note (gap record — parity-only
   cleanup known difference).
5. **Deacon extensions from shipped features**: workspace-trust gate (016 host CA
   injection settings, 017 user profiles, 015 auto-forward-port daemon registry files —
   each an `ext-` record so they are never misreported as divergences).

Also seeded: the four source-revision records (`rev-spec-113500f4`,
`rev-schema-113500f4`, `rev-oracle-0-87-0`, `rev-cli-surface` pinned to the oracle
version), context dimensions/values, the six observable channels, the initial profile
`prof-linux-amd64-docker-0870`, and behavior/case records for the existing tier1 corpus
cases and live parity binaries (each live binary's scenario → case records referencing the
test binary by name).

**Rationale**: This is the complete set of currently *documented* divergences (SC-001).
The oracle pin already reads 0.87.0, so no pin alignment work is needed — the registry's
`rev-oracle-0-87-0` cross-checks `fixtures/parity-corpus/oracle.json` at validation time
(stale-pin rule). Note: CLAUDE.md references `docs/subcommand-specs/*/SPEC.md`, which no
longer exists in the tree; the registry does not cite it as a source revision.

**Alternatives considered**: exhaustively enumerating all spec clauses day one (rejected:
spec Assumptions explicitly make inventory population incremental; the day-one mandate is
divergences + existing empirical coverage).

## Decision 7: Reports generated to `target/conformance/`, not checked in

**Decision**: `conformance report` writes `target/conformance/report.json`
(machine-readable) and `target/conformance/report.md` (human-readable), directory
overridable via `--out-dir`. Reports are build artifacts, not committed files. All record
iteration is sorted by stable ID before serialization; no timestamps, hostnames, or
absolute paths appear in `report.json` (SC-004 byte-identical). The release workflow
uploads both as release artifacts; `certify` gates the release.

**Rationale**: A committed report is a drift liability requiring a freshness CI check —
generation-on-demand from validated data is strictly simpler and equally traceable.
Mirrors the existing `target/parity/` convention (`DEACON_PARITY_REPORT_DIR` precedent).

**Alternatives considered**: committed `docs/CONFORMANCE.md` with a CI freshness gate
(rejected: adds the exact duplicate-representation problem this feature removes).

## Decision 8: CI wiring

**Decision**: A hermetic test in `crates/conformance/tests/` (`registry_valid`) runs
`validate` against the real `conformance/registry/` on every PR — it needs no new nextest
group (no Docker, light filesystem; default groups suffice, and it lands in `dev-fast`
automatically since that profile excludes only enumerated docker/smoke binaries). Strict
certification runs as a blocking step in `.github/workflows/release.yml`'s verify job
(`cargo run -p deacon-conformance -- certify`), per clarification Q5. The crate must
compile on the Windows `dev-fast` lane: no Unix-only APIs; path assertions
separator-agnostic.

**Rationale**: Per-PR structural validation piggybacks on existing test lanes (zero new
workflow surface); release gating touches exactly one workflow.

**Alternatives considered**: dedicated conformance workflow (rejected: more CI surface
for no isolation benefit — validation is hermetic and fast).

## Decision 9: Case records reference executable tests by nextest identity

**Decision**: A `case-` record references its executable test as
`{ "binary": "<test binary name>", "test": "<optional test function filter>" }` plus, for
corpus-driven binaries, the corpus case directory (`{"corpus": "tier1", "case":
"extends-child"}`). Validation checks referential integrity *within the registry*;
existence of the named test binary is checked against `crates/*/tests/` file names (same
structural technique `parity-harness::registry::check_test_files` already uses) so a
deleted test fails validation as an invalid reference.

**Rationale**: Clarification Q2 — the registry is a linkage layer. Reusing the proven
registry↔tests↔nextest structural-check pattern keeps "orphaned case" and "dangling
executable reference" both enforceable hermetically.

**Alternatives considered**: free-text test references (unverifiable — silent rot);
executing tests to verify existence (violates hermetic/no-runtime constraints).

## Deferred decisions

None. All deferrals, if any emerge during implementation, must be added to `tasks.md`
under "## Deferred Work" per constitution I.
