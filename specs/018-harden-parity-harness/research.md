# Research: Harden the Parity Test Harness

**Feature**: 018-harden-parity-harness | **Date**: 2026-07-19

Inputs inspected before these decisions, per the spec's planning mandate:

- **Current repository**: all 8 `crates/deacon/tests/parity_*.rs` binaries and
  `parity_utils.rs` (981 lines; `gated()` at L337 silently converts unmet
  prerequisites into passes); the three Python corpus runners
  (`run_tier1.py` L113 and `run_tier1_merged.py` L108 return `None` → always exit
  0; only `run_tier1_errors.py` L166 exits nonzero); `.config/nextest.toml`
  (7 profiles; `ci`/`mvp-integration` select parity binaries that then self-skip
  green); `Makefile` `test-parity` (invokes `cargo test`, not nextest — a
  side-channel contract); `.github/workflows/` (no workflow installs the oracle or
  sets `DEACON_PARITY=1` — parity is dead in CI).
- **Pinned reference implementation**: `@devcontainers/cli@0.87.0` confirmed
  published on npm (tarball `cli-0.87.0.tgz`); `devcontainer --version` prints the
  bare version string; not installed in this dev environment (which is exactly the
  state that today yields vacuous passes).
- **containers.dev specification**: upstream commit `113500f4` per constitution I.
  The spec defines behavior for the *product*, not for test harnesses — the
  workspace-trust gate precedent (SECURITY.md) establishes that deacon-specific
  infrastructure hardening is permitted and documented as deacon-specific. Nothing
  here changes spec-governed behavior; characterized divergences (`deacon-stricter`)
  remain as recorded intent.

---

## Decision 1 — Oracle pin location: `fixtures/parity-corpus/oracle.json`

**Decision**: One machine-readable pin file, `fixtures/parity-corpus/oracle.json`:
`{ "package": "@devcontainers/cli", "version": "0.87.0" }`. Read by the harness
crate (compile-time `include_str!` + parse, so a malformed pin fails every parity
test loudly), by the CI workflow (`jq -r .version`), and by the Makefile alias.

**Rationale**: FR-001 demands a single authoritative, harness-read location. The
corpus directory is already the home of parity ground truth (README, REPORT,
errors corpus), so the pin lives next to what it certifies. JSON so CI and make
read it without a Rust build.

**Alternatives considered**: Rust `const` in the harness crate (not readable by
CI/make without building); root-level `PARITY_ORACLE` file (orphaned from the
corpus it governs); pinning only in the workflow YAML (harness couldn't verify —
the exact failure mode being fixed).

## Decision 2 — Selection by nextest profile, not env-var opt-in

**Decision**: Add `[profile.parity]` to `.config/nextest.toml` whose
`default-filter` selects exactly the live parity binaries. Remove parity binaries
from the *selection* of `default`, `full`, `docker`, `ci`, and `mvp-integration`
(they are already excluded from `dev-fast`). Delete the `DEACON_PARITY=1` opt-in
entirely: if a parity test runs, it runs for real; environments that shouldn't run
it don't select it (FR-006, FR-014). `parity_registry_check` and
`parity_harness_faults` are hermetic (no oracle/Docker) and are selected in the
regular lanes too, so the trust property is guarded on every PR.

**Rationale**: The env-var gate is precisely the mechanism that lets a selected
test decide not to test and still report green. nextest selection makes "not run"
visible in lane output and truthful in status (SC-007). This mirrors how
`dev-fast` already excludes docker/smoke by filter.

**Alternatives considered**: keep `DEACON_PARITY` and make it fail-instead-of-skip
when unset (still executes-and-fails everywhere the profile selects it — turns
every existing lane red); nextest `platform`-style conditional skips (nextest has
no first-class runtime-prereq skip, and a skip would still misreport coverage).

## Decision 3 — Harness crate owns prereqs, version verification, bounded execution

**Decision**: New `crates/parity-harness` (publish = false, dev-dependency of
`deacon`). Core API:

- `Oracle::acquire()` — resolves the binary (`DEACON_PARITY_DEVCONTAINER` override,
  else `PATH`), runs `--version` under the 2-minute bound, compares **exactly**
  against the pin; caches the verified result in a process-wide `OnceLock`. Any
  failure returns a `HarnessError` variant naming found-vs-required version and the
  resolution path; test binaries `.expect()` it so the test FAILS with that
  message (never returns early).
- `require_docker()` / `require_fixture(path)` — hard, cause-specific errors.
- `exec_oracle(kind, args)` / `exec_deacon(kind, args)` — `tokio::process` +
  `tokio::time::timeout` (Config = 2 min, Lifecycle = 15 min per clarification),
  capturing raw stdout/stderr always; timeout → `HarnessError::OracleTimeout` with
  partial output preserved.

`parity_utils.rs` is deleted; its comparison logic moves into the crate, its
`gated()`/`upstream_available()` silent-skip pattern is not carried over.

**Rationale**: FR-002..FR-007. A crate (vs a `tests/` include-module) gets unit
tests, doctests, clippy/fmt as a first-class target, and can host the aggregator
binary (D8). `OnceLock` keeps version verification to one subprocess per test
binary. Keeping the `DEACON_PARITY_DEVCONTAINER` override preserves the documented
local workflow and gives fault-injection tests their seam (D10).

**Alternatives considered**: extend `parity_utils.rs` in place (no unit-test/lint
surface; recompiled per binary; cannot host a bin target); verify version once in
the Makefile/CI only (leaves the harness trusting its caller — the wrong-oracle
fault would pass undetected when invoked any other way).

## Decision 4 — Port the three Python corpus runners to Rust test binaries

**Decision**: Three new test binaries — `parity_corpus_tier1.rs`,
`parity_corpus_merged.rs`, `parity_corpus_errors.rs` — each one `#[test]` that
discovers cases (manifest else directory scan, as today), runs both CLIs via the
harness, classifies per-case results, writes the report fragment, and fails with a
per-case summary if any case has an unwaived divergence, a process failure, a
normalization failure, or the discovered count is below the registry minimum
(FR-009, FR-024). The Python scripts are deleted; `fetch_realworld_corpus.py`
stays (corpus-fetch utility, not a comparison runner; it makes no pass/fail
claim).

**Rationale**: Clarification session fixed "port, not wrap". Wrapping would keep a
second normalization implementation alive in Python, violating FR-019. Porting
lets all three runners share `parity_harness::normalize`/`waiver`/`report`
verbatim. One `#[test]` per runner (not test-per-case) because case discovery is
runtime-dynamic and per-case subtests would need codegen; per-case detail lives in
the report fragment and failure message instead.

**Alternatives considered**: shell-out wrapper tests (keeps Python normalization →
FR-019 violation; exit-code laundering); build-script-generated test-per-case
(corpus changes would silently drop cases between generations — the class of bug
this feature kills); a single `parity_corpus.rs` binary (loses per-runner nextest
grouping/timeout control and per-runner report identity).

## Decision 5 — Parity registry: `fixtures/parity-corpus/registry.json`

**Decision**: A registry file enumerating (a) every live parity binary, (b) every
corpus with its minimum expected case count, (c) the reclassified
internal-consistency binaries (listed so the completeness check can assert they are
NOT in the parity profile). `parity_registry_check.rs` — hermetic, selected in all
lanes — verifies: every registered binary exists as a `tests/*.rs` file; every
`parity_*` file is registered; `.config/nextest.toml`'s parity profile filter
covers exactly the registered live set; no parity source contains `#[ignore]` or
the legacy silent-skip idioms (`gated(`, `upstream_available(`, bare
`return;`-on-prereq patterns — asserted via source scan); corpus dirs meet their
minimum counts. The aggregator (D8) additionally verifies at certification time
that a fragment exists for every registered binary — catching "registered but
never executed" (FR-022).

**Rationale**: FR-022/FR-023/SC-003. Splitting the check in two gives cheap
every-PR structural guarantees (file/date/filter drift) plus runtime execution
proof in the lane that actually runs parity. Data-file registry (vs Rust const)
lets the CI workflow and the aggregator share it without recompiling.

**Alternatives considered**: derive the registry implicitly from the file-name
glob (can't encode min case counts or the live-vs-consistency split, and a deleted
binary would silently shrink coverage — the exact rot FR-022 forbids); registry as
Rust const in the harness crate (aggregator could use it, but CI YAML could not,
and edits wouldn't be reviewable as data).

## Decision 6 — Unified waiver model: one schema, one loader, records adjacent to cases

**Decision**: One `Waiver` record schema (see data-model.md): `id`, `scope`
(corpus case or observable-state field pattern), `expect` (kind:
`both-reject` | `both-accept` | `deacon-stricter` | `field-divergence` with
expected values), `rationale`, `added` (date). Loader + staleness validation live
in `parity_harness::waiver`. Existing `errors/*/expect.json` files are kept in
place and validated against the schema (they already carry the `expect` kinds).
Observable-state's `KNOWN_INTENTIONAL_DIVERGENCES` / `KNOWN_GAPS` Rust consts
(both currently empty) are deleted; any future observable-state waiver is a JSON
record under `fixtures/parity-corpus/waivers/`. Staleness (FR-011): every run,
each waiver in scope must (a) match an existing case/field and (b) have its
expected difference actually observed; otherwise the run fails naming the stale
record. Applied waivers are listed per case in the report fragment (FR-010).

**Rationale**: Clarification fixed the shape (single schema + shared loader,
records adjacent to cases). Both current mechanisms map cleanly: `expect.json` is
already adjacent-per-case; the Rust const lists are empty, so migrating them is
free now and only gets more expensive later.

**Alternatives considered**: one central waiver registry file (drifts from the
cases it describes; merge-conflict magnet; rejected in clarification); keeping
both mechanisms documented separately (the same difference could be waived in one
runner and fatal in another — exactly FR-012's complaint).

## Decision 7 — Normalization: single module, prune-based config semantics win

**Decision**: `parity_harness::normalize` becomes the only equivalence definition,
with one entry point per comparison type: `config` (Tier 1 / read-configuration),
`merged_config` (Tier 1b), and `container_state` (observable-state). For config,
the **Python `prune` semantics** are ported (unwrap `{configuration}`, drop
`configFilePath`, prune nulls/empty containers, sanitize dynamic values like
`${devcontainerId}` hashes) and the Rust `extract_core_config` ~12-key
**allowlist is retired**: comparing only an allowlist silently ignores
divergences in every non-listed key, which is a permissive fallback (FR-006's
spirit) — full-shape compare with documented pruning is the hardened form.
Container-state normalization (noise-env subtraction, label prefixes, project
prefix stripping, user normalization) moves over from `parity_utils.rs` as-is —
it is the sole implementation already. Normalization functions return
`Result`; any internal failure is `HarnessError::Normalization` → test failure
(FR-005), never a fallback to raw comparison.

**Rationale**: FR-019/SC-005. Three copies (Rust allowlist, Python prune ×2 files)
disagree today; the strictest semantically-honest one becomes canonical. Raw
outputs are preserved regardless (D8), so widening the compared surface costs
diagnosis nothing.

**Alternatives considered**: keep the allowlist for the Rust binaries "because it
is stable" (institutionalizes two verdicts per comparison type — SC-005
violation); normalize via a JSON-canonicalization external crate (adds a
dependency for what ~150 lines of owned, unit-tested code does; constitution
dependency-hygiene).

## Decision 8 — Report fragments per binary + aggregator bin; raw outputs always captured

**Decision**: Each parity test binary writes one JSON fragment to
`target/parity/report/<binary>.json` (dir overridable via
`DEACON_PARITY_REPORT_DIR`; writes are temp-file + `fs::rename` atomic, per the
repo's durable pattern). Fragment: binary id, oracle {version, path}, per-case
{id, mode: live, outcome, cause-on-failure, waivers-applied, raw-output paths}.
Raw stdout/stderr of both CLIs are written under
`target/parity/raw/<binary>/<case>/` for every comparison (pass or fail —
FR-020's "reproducibly obtainable" is satisfied by always-on capture; retention
is the lane's concern). A `parity-report` bin in the harness crate aggregates
fragments → `target/parity/parity-report.json`, cross-checks the registry
(fragment present for every registered live binary; every corpus met its
minimum; no stale waivers), and exits nonzero on any gap (FR-016, FR-018,
FR-022). CI uploads `target/parity/` as the run artifact.

**Rationale**: nextest runs binaries in parallel with no ordering, so a
shared-file report would race; per-binary fragments + post-run aggregation is the
contention-free shape. The aggregator doubles as the "was everything actually
executed" gate, which a test inside the run cannot be (it can't know the run's
own final selection). Failure to write a fragment fails that binary's test
(FR-018); a missing fragment fails aggregation.

**Alternatives considered**: parse nextest's own JSON output (proves pass/fail
but knows nothing of oracle version, waivers, or raw paths); one test binary
that runs everything serially (loses nextest grouping/parallelism and the
per-binary timeout config); libtest JSON events (unstable interface).

## Decision 9 — Reclassify the two oracle-free "parity" binaries

**Decision**: `parity_remote_env_flags.rs` → `consistency_remote_env_flags.rs`,
`parity_env_probe_flag.rs` → `consistency_env_probe_flag.rs` (git `mv`, content
unchanged except header comment). They leave the `parity`/`parity-cli` nextest
groups, get regular unit/integration grouping, stay selected in the fast lanes
(they are hermetic and valuable), and are listed in the registry under
`internal_consistency` so the registry check asserts they never re-enter the
parity profile (FR-013).

**Rationale**: They never invoke the oracle; their names currently inflate
apparent parity coverage — the exact overstatement FR-013/US2-5 targets.

**Alternatives considered**: leave names, exclude from profile (name still lies
in every report and grep); give them oracle comparisons (out of scope per spec
assumption — they test deacon-internal flag consistency, which has no oracle
analogue).

## Decision 10 — Fault-injection acceptance tests via executable stubs

**Decision**: `parity_harness_faults.rs` (hermetic, all lanes, `#[cfg(unix)]` for
stub-script cases with a one-line reason, matching repo precedent) plus harness
unit tests cover FR-021:

- *wrong oracle version*: stub `devcontainer` script printing `0.86.0`;
  `DEACON_PARITY_DEVCONTAINER` → stub; assert `Oracle::acquire()` fails naming
  found (0.86.0) and required (0.87.0).
- *missing oracle*: override pointing at a nonexistent path + empty `PATH` seam;
  assert cause names absence + provisioning hint.
- *missing Docker*: `require_docker()` with a `DEACON_PARITY_DOCKER` override
  pointing at a failing stub; assert Docker named as the missing prerequisite.
- *oracle crash / malformed output*: stubs exiting nonzero mid-protocol and
  emitting non-JSON; assert `OracleFailure` / `MalformedOutput` causes.
- *injected output difference*: feed two fabricated JSON documents differing in
  one key through the comparison pipeline; assert unwaived-divergence failure;
  add a matching waiver record fixture; assert pass-with-waiver-reference; assert
  stale-waiver failure when the difference is absent.
- *normalization failure*: invalid input into `normalize::config`; assert
  `Normalization` cause, no fallback.
- *timeout*: stub sleeping past a test-shortened bound (bound injectable for
  tests); assert `OracleTimeout` with partial output preserved.

The "every registered binary is executed" acceptance proof is the aggregator gate
(D8) exercised in the certification lane, plus `parity_registry_check` (D5)
structurally on every PR.

**Rationale**: FR-021/SC-001 demand *proof* the harness cannot lie, hermetically
(constitution: deterministic, no network). Stub executables via the existing
override env vars test the real resolution/execution code paths rather than
mocked abstractions.

**Alternatives considered**: testing against a real second oracle version
(network + npm in unit lanes — forbidden); mocking at a trait seam only (leaves
the actual subprocess/override path untested — the path that rotted last time).

## Decision 11 — Certification lane: `.github/workflows/parity.yml`

**Decision**: New workflow, triggers: `schedule` (nightly, main),
`workflow_dispatch`, and `pull_request` with `paths:` covering
`crates/parity-harness/**`, `crates/deacon/tests/parity_*`,
`crates/deacon/tests/consistency_*`, `fixtures/parity-corpus/**`,
`.config/nextest.toml`, `Makefile`, and the workflow file itself. Steps: checkout;
Node 20; `npm install -g @devcontainers/cli@$(jq -r .version
fixtures/parity-corpus/oracle.json)`; `devcontainer --version` echo (harness
re-verifies — the workflow check is belt, harness is suspenders); build deacon
release binary; `cargo nextest run --profile parity`; `cargo run -p
parity-harness --bin parity-report`; upload `target/parity/` as artifact
(default retention). The job fails on any nonzero step — there is no
`continue-on-error` anywhere in it. Not added to required PR checks for
unrelated changes (clarification).

**Rationale**: FR-015 + clarified cadence. Path-triggered PR runs protect the
harness/corpus/pin themselves; nightly catches drift (oracle repub, Docker image
changes); dispatch supports release certification. Provisioning installs from the
pin file so workflow and harness can never disagree about the version.

**Alternatives considered**: required check on every PR (15-min-class container
comparisons on unrelated docs PRs — disproportionate; rejected in
clarification); reusing `ci.yml` with a job (separate workflow keeps status
naming unambiguous per FR-017 — the check appears as "parity / live-certification",
distinguishable at a glance).

## Decision 12 — `make test-parity` becomes a thin alias

**Decision**: `test-parity` body becomes: verify pin file exists, then
`cargo nextest run --profile parity` followed by
`cargo run -p parity-harness --bin parity-report`; `test-parity-all` stays an
alias. All resolution/version/gating logic leaves the Makefile (the harness owns
it). The current `cargo test --test … --test-threads=1` invocation and the
`DEACON_PARITY*` env plumbing are removed; `DEACON_PARITY_UPSTREAM_READ_CONFIGURATION`
template plumbing moves into the harness with a sane default.

**Rationale**: Clarified Q4; constitution III's nextest-only standard; removes the
last side-channel contract. The Makefile keeping even an oracle-existence check
would duplicate the harness's authority (two sources of truth for one gate).

**Alternatives considered**: delete the target (breaks documented workflow and
README; rejected in clarification); keep `cargo test` fallback path (recreates
the dual-contract drift this feature removes).

---

## Deferred decisions

None. All spec clarifications are resolved; no NEEDS CLARIFICATION markers remain
in the Technical Context. (Raw-artifact retention beyond CI defaults and any
future snapshot/replay mode are explicitly out of scope per the spec.)
