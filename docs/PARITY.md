# Deacon Conformance & Parity Tracker

**Pins:** upstream [devcontainers/spec](https://github.com/devcontainers/spec) commit `113500f4` (schemas + normative prose) · reference oracle `@devcontainers/cli` **0.87.0** (exact-version verified before any live comparison).

**Source of this document:** the repository-owned conformance registry (`conformance/registry/`), live `deacon-conformance certify` / `coverage report` output, and open GitHub issues. Every claim traces to a committed record; nothing here is aspirational.

**Regenerate the counts:**

```bash
cargo run -q -p deacon-conformance -- certify
cargo run -q -p deacon-conformance -- coverage report   # → target/conformance/
```

## Legend

Every behavior record carries **three independent axes** (`conformance/RULES.md`):

| Axis | Values | Meaning |
|---|---|---|
| `spec` | conformant / nonconformant / unspecified / not-applicable | relation to the written spec |
| `reference` | aligned / divergent / unknown / not-applicable | relation to the **observed** pinned reference CLI |
| `decision` | follow-spec / align-with-reference / deacon-extension / intentional-divergence / unresolved-gap | what deacon decided to do |

There is deliberately no combined "different but acceptable" state. Statuses are **evidence-backed claims**: a behavior may only claim `aligned`/`divergent` if a test case or waiver stands behind it (contradiction rules R1–R8; R8 in particular forces `reference: unknown` when neither exists).

**Vocabulary**

- **Divergence** — a *characterized* difference: we know what both sides do and why. Either fix-tracked (deacon behind) or accepted (`intentional-divergence` + waiver, or `deacon-extension`).
- **Gap** (`gap-`) — an *admission of missing coverage*. No evidence stands behind it. **Always blocks release certification.**
- **Waiver** (`wvr-`) — an accepted difference with a mandatory `expires`. The harness verifies it *keeps reproducing*: a difference that stops reproducing fails as **stale**. So a waiver is positive evidence, and it never blocks.
- **Residual** (`res-`) — missing *representation*, not missing coverage: the coverage exists in a legacy program not yet retired. Never blocks certification; blocks deleting its carrier.
- **Out of scope** — differences with no observable effect are recorded nowhere, by rule.

**Evidence strength** (weakest → strongest)

| Kind | Compares against the reference? |
|---|---|
| `spec-expectation` | **No** — pins deacon's own behavior only |
| `invariant-metamorphic` | **No** — relates ≥2 deacon runs (idempotence, restart) |
| `snapshot` | Yes — vs a committed, provenance-checked recording |
| legacy `parity_*` binary | Yes — live, but hand-written |
| `live-differential` | Yes — deacon and the verified oracle side by side, per channel |
| waiver | Yes — and self-invalidates when the difference stops reproducing |

## Summary

| Metric | Value |
|---|---|
| Behavior records | **71** across 16 areas |
| — `follow-spec` / `align-with-reference` | 37 / 6 |
| — `intentional-divergence` / `deacon-extension` | 14 / 14 |
| — `unresolved-gap` | **0** |
| Reference axis: aligned / divergent / not-applicable / unknown | 36 / 23 / 12 / **0** |
| Test cases | **231** — 109 live-differential, 108 spec-expectation, 11 legacy, 2 metamorphic, 1 snapshot |
| Waivers (all with expiry, self-invalidating) | 21 |
| Extension records (`ext-`) | 10 |
| Open gap records | **6** — `gap-pairwise-{build,down,exec,run-user-commands,up,upgrade}` |
| Scenario obligations | 743 — covered 444, waived 1, gap 298, undispositioned **0** |
| Residuals | 8 queued + 6 permanent (61 units, structurally outside the model) |
| Certification verdict | **NOT certified — by design.** The 6 gap records block, as intended. |

### Pairwise scenario coverage per operation

| Operation | Covered | Operation | Covered |
|---|---|---|---|
| read-configuration | 66 / 66 ✅ | run-user-commands | 31 / 112 |
| outdated | 66 / 66 ✅ | down | 26 / 55 |
| templates-apply | 25 / 25 ✅ | exec | 26 / 76 |
| doctor | 24 / 24 ✅ | upgrade | 25 / 47 |
| up | 54 / 114 | build | 31 / 87 |

## Per-area status

**Source**: Spec = written spec text · Ref = reference-CLI contract with no spec text · Ext = deacon extension.
**Permanent?**: waived divergences carry a re-review date; fix-tracked items do not.

### read-configuration (25 behaviors — the deepest-covered area)

| Behavior | Source | Status | Permanent? | Evidence | Tracking |
|---|---|---|---|---|---|
| Basic parse & echo | Spec | Parity | — | live-diff + snapshot | — |
| Config discovery incl. `.devcontainer/<folder>/` | Spec | **Reference behind spec** (0.87.0 skips the nested folder without `--config`) | Waived → 2027-07-26 | live-diff + waiver | — |
| Missing config / bad `--config` rejected | Spec | Parity (both reject) | — | live-diff + waivers | — |
| Unknown top-level fields preserved | Spec | Parity | — | live-diff + waiver | — |
| Duplicate JSON keys: last wins | Spec | Parity | — | live-diff + waiver | — |
| Tier-1 corpus (24 real-world shapes) | Spec | **Deacon behind** — residual `featuresConfiguration` shape; 5 of 6 original divergence families fixed | No — fix tracked | live-diff ×24 | **#387** |
| `--include-merged-configuration` (24 variants) | Spec | **Deacon behind** — same family | No — fix tracked | live-diff ×24 | **#387** |
| `featuresConfiguration` document shape | Spec | **Deacon behind** | No — fix tracked | spec-exp | **#387** |
| `configFilePath` as VS Code URI object | Ref | Parity (fixed) | — | live-diff | — |
| Merged lifecycle slots emit `[]` when empty | Ref | Parity (fixed) | — | live-diff | — |
| `featuresConfiguration` omitted when none resolve | Ref | Parity (fixed) | — | live-diff | — |
| `portsAttributes` authored keys only | Ref | Parity (fixed) | — | live-diff | — |
| `workspace` section shape | Ref | Parity (fixed, #383) | — | live-diff | — |
| `workspaceFolder` preserves git-root subdir | Spec | Parity (fixed, #383) | — | live-diff | — |
| Substitution in object-shaped fields | Spec | Parity | — | ⚠️ spec-exp only | — |
| Malformed JSONC rejected (reference: lenient) | Ref | Intentional divergence — fail fast | Yes → 2027-01-19 | live-diff + waiver | — |
| Wrong-type `features` / `forwardPorts` rejected | Ref | Intentional divergence | Yes → 2027-01-19 | live-diff + twins + waivers | — |
| Unsupported enum values rejected | Ref | Intentional divergence | Yes → 2027-07-26 | live-diff + waiver | — |
| Authored-null vs omitted collapsed | Ref | Intentional divergence (residue; empty half fixed) | Waived → 2027-01-26 | live-diff + waiver | #398 |
| Merged computed empties omitted | Ref | Intentional divergence | Waived → 2027-01-31 | live-diff + waiver | — |
| `extends` resolved eagerly; missing/cycle rejected (3) | Ext | Deacon extension | Yes → 2027-01-19 | spec-exp + waivers | `ext-extends-resolution` |

### up (12 behaviors)

| Behavior | Source | Status | Evidence | Tracking |
|---|---|---|---|---|
| `up` + `exec` end-to-end | Spec | Parity | live-diff + legacy + metamorphic | — |
| containerEnv/remoteEnv precedence | Spec | Parity | live-diff + spec-exp | — |
| Feature install order (dependency-sorted) | Spec | Parity | live-diff + metamorphic | — |
| Feature install failure surfaces | Spec | Parity | live-diff | — |
| Feature entrypoint chaining | Spec | Parity | live-diff + spec-exp | — |
| Mount source and shape | Spec | Parity | live-diff + spec-exp | — |
| Container PATH construction | Spec | Parity | live-diff + spec-exp | — |
| Lifecycle command forms | Spec | Parity | live-diff + spec-exp | — |
| Lifecycle cwd = workspace folder | Spec | Parity | ⚠️ spec-exp only | — |
| Effective user uid/gid | Spec | Parity | ⚠️ spec-exp only | — |
| Restart reuses container | Spec | Parity | ⚠️ metamorphic + spec-exp (deacon-only) | — |
| Changed config recreates container (reference reattaches to stale) | Ref | Intentional divergence — safer branch | spec-exp + waiver → 2027-01-26 | adjacent #371 |

### build (3) · exec (4) · run-user-commands (2)

| Behavior | Source | Status | Evidence | Tracking |
|---|---|---|---|---|
| build: image parity incl. compose | Spec | Parity | live-diff + legacy | — |
| build: features layered into `--image-name` | Spec | Parity | live-diff + spec-exp | — |
| build: failures reported | Spec | Parity | live-diff ×3 | — |
| exec: command parity (tty, user, cwd, compose) | Spec | Parity | live-diff ×9 + legacy | — |
| exec: image PATH preserved | Spec | Parity | live-diff | — |
| exec: container-id metadata read-back | Spec | Parity | ⚠️ legacy binary is sole evidence | — |
| exec: restored PATH ordering differs from reference | Ref | Intentional divergence | ⚠️ spec-exp only, **no waiver** | see weak spots |
| run-user-commands: hook order | Spec | Parity | live-diff + spec-exp | — |
| run-user-commands: hook failure propagates (reference exits 0) | Spec | **Reference behind spec** | live-diff + twins + waiver → 2027-01-26 | — |

Not yet a behavior record: **#405** (run-user-commands ignores image/container `devcontainer.metadata` while `exec` honors it).

### outdated (4) · upgrade (3)

| Behavior | Source | Status | Evidence | Tracking |
|---|---|---|---|---|
| outdated: reports Feature versions (lockfile-aware) | Spec | Parity | live-diff ×4 + spec-exp | — |
| outdated: keyed by declared reference (was: collision dropped a Feature) | Ref | Parity (fixed) | ⚠️ spec-exp only | #407 |
| outdated: extends-chain Features reported | Spec | **Reference behind spec** (consequence of the extension) | spec-exp + waiver → 2027-07-26 | — |
| outdated: malformed lockfile rejected | Ref | Intentional divergence (reference never reads it) | spec-exp + waiver → 2027-01-31 | #406 |
| upgrade: regenerates lockfile | Spec | Parity | live-diff + spec-exp | — |
| upgrade: empty Feature set → empty lockfile, exit 0 (reference errors) | Spec | Intentional divergence | live-diff + waiver → 2027-07-26 | — |
| upgrade: honors `--override-config` / `--merge-config` | Ext | Deacon extension | spec-exp | #409 |

Not yet a behavior record: **#389** (`--output` vs reference's `--output-format`).

### observable-state (8 behaviors)

| Behavior | Source | Status | Evidence | Tracking |
|---|---|---|---|---|
| Container state parity (labels, config, network) | Spec | Parity | legacy + spec-exp | — |
| Normalized state diff parity | Spec | Parity | live-diff ×4 + legacy | — |
| `devcontainer.metadata` content | Spec | Parity (fixed) | live-diff | #373 |
| `devcontainer.metadata` byte serialization | Ref | Intentional divergence | live-diff + waiver → 2027-01-29 | **#394** (ordering nondeterminism is a bug within this row) |
| Compose project file set (stdin `-` vs temp file) | Ref | Intentional divergence | live-diff + waiver → 2027-01-26 | — |
| Compose project name always docker-valid | Ref | Intentional divergence — robustness | live-diff + legacy + spec-exp | #265 |
| Keepalive command form (BusyBox-safe) | Ref | Intentional divergence | live-diff ×5 | — |
| Five extra deacon identity labels | Ext | Deacon extension | live-diff (scoped per-label tolerances) | `ext-container-identity-labels` |

Not yet a behavior record: **#399** (empty-string `dockerComposeFile` takes different provisioning paths).

### Deacon extensions with no reference equivalent

The reference has no surface at all for these, so `reference: not-applicable` is a classification, not an unverified claim. Evidence is deacon-only by necessity.

| Capability | Evidence | Record |
|---|---|---|
| `down` — teardown (3 behaviors) | spec-exp ×7 | `ext-teardown-command` |
| `doctor` — diagnostics, human + JSON | spec-exp ×9 | `ext-doctor-diagnostics` |
| Auto-forward ports daemon | legacy integration | `ext-auto-forward-ports` |
| `.env`-format secrets file (superset) | legacy integration | `ext-secrets-file-env-format` |
| Workspace-trust gate on host hooks | legacy integration | `ext-workspace-trust-gate` |
| Host CA injection | legacy integration | `ext-host-ca-injection` |
| User profiles from settings.json | legacy integration | `ext-user-profiles` |
| `--merge-config` on config-consuming subcommands | spec-exp | `ext-cli-config-overlay` |

### templates-apply (1 behavior)

Scaffolds a template with option substitution — parity claimed against the spec, but **no reference comparison is structurally possible**: the reference's `--template-id` accepts only OCI refs while deacon takes the template positionally, so no shared argv exists. Evidence: spec-expectation ×8 over the scaffolded bytes.

## Known weak spots

Called out deliberately — this section is the point of the document.

1. **`aligned` claims resting on spec-expectation evidence only.** Nothing compares these against the reference on any run, so oracle drift would go undetected:
   - `bhv-up-effective-user-uid-gid`, `bhv-up-lifecycle-command-cwd`
   - `bhv-readconfig-substitution-object-fields`
   - `bhv-outdated-reports-declared-reference-key`
   - `bhv-up-restart-reuses-container` — evidence is a metamorphic relation across two *deacon* runs; the reference never enters it.
2. **Single-carrier evidence.** `bhv-exec-container-id-metadata` is evidenced only by the legacy `parity_up_exec` binary (on record; blocks that binary's retirement).
3. **An intentional divergence with no self-invalidating backing.** `bhv-exec-restored-path-ordering` is permitted by R8 (a case backs it), but its only case is `spec-expectation` — so if the reference changed to match, nothing would report the waiver-style *stale* signal. Its two unwaivered siblings are fine by contrast: `bhv-container-keepalive-command` has 5 live differentials and `bhv-compose-project-name-robust` has a live legacy carrier.
4. **Thin or absent live-differential coverage.** `templates-apply` (zero, structurally impossible); `outdated` 4 of 20 cases live; `upgrade` 3 of 11. `down`/`doctor` have zero, but they are extensions with no reference side. The extension areas run on deacon-only integration tests.
5. **Snapshot coverage is one case on one platform** (`case-readconfig-snapshot`, linux-x86_64). Other platforms report `no-reference-for-platform` — a declared coverage limit, not a silent skip.
6. **The measured scenario hole.** 298 of 745 obligations are open gaps, concentrated in `run-user-commands` (81 uncovered pairs), `up` (60), `build` (56), `exec` (50), `down` (29), `upgrade` (22).
7. **Corpus divergence history is fix-tracked, not hidden.** At migration time 51 of 71 read-configuration corpus cases diverged across six families. Five are fixed; the residual `featuresConfiguration` shape keeps `reference: divergent` on those records, tracked in #387.

## Open gaps (block release certification)

`gap-pairwise-{build, down, exec, run-user-commands, up, upgrade}` — enumerated pairwise scenario combinations no declarative case covers. Live counts in `target/conformance/coverage-pairwise.md`. Each new case re-dispositions the pairs it covers; the record is deleted when none remain.

`certify` reports **not certified** until these close. The model measures the hole rather than certifying around it.

## Open issues

| Issue | Area | Summary | Registry status |
|---|---|---|---|
| #387 | read-configuration | `featuresConfiguration` structurally differs | characterized, fix pending |
| #394 | observable-state | metadata label bytes nondeterministic | waiver covers whitespace; ordering fix pending |
| #398 | read-configuration | authored-null vs omitted collapsed | record + waiver for the residue |
| #399 | up/compose | empty-string `dockerComposeFile` | not yet a record — triage |
| #405 | run-user-commands | ignores image/container metadata; `exec` honors it | not yet a record — triage |
| #389 | outdated | `--output` vs `--output-format` | not yet a record — triage |
| #376 | cross-area | triage umbrella for 024-surfaced divergences | in progress |
| #371 / #372 | up / run-user-commands | stale container left running; markers with `config_hash: None` | bugs adjacent to characterized divergences |

Recently closed: #406, #407, #409, #370, #373, #383.

In flight: **#411** — a child's Feature version overriding its base's across an `extends` merge, with the tie-less single-document form rejected as its floor. Landing separately; it will add two behavior records, two cases and one waiver to the counts above.

---

*Verification pipeline: hermetic registry validation (`registry_valid`, classes V1–V36) on every PR · the `parity / live-certification` lane (nightly + parity-path PRs) runs the 109 live differentials against the verified oracle · `certify` gates releases on gaps and uncovered behaviors · waivers self-invalidate when their difference stops reproducing · the nightly `discovery` lane searches for differences nobody curated and gates nothing.*
