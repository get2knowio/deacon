# Research: Normative Clause Inventory

All Technical Context unknowns resolved. This feature is the **prose companion** to
feature 020 (schema constraint inventory) and reuses its machinery, ID grammar, and
violation-class engine in the dev-only `deacon-conformance` crate. The central new
problem — absent from 020 — is that prose clauses cannot be extracted by a deterministic
parser the way JSON-Schema constraints can. Decisions 1–2 resolve that tension; the rest
mirror 020 with prose-specific adaptations (moves, strength detection, ambiguity).

The pinned source is `devcontainers/spec` @ `113500f4` (the registry's
`rev-spec-113500f4`). Per the `/speckit.clarify` session, the inventoried surface is the
ratified `docs/specs/` document set — consumer AND authoring — with authoring documents
inventoried in full and classified not-applicable; draft `proposals/` are out of scope.

## Decision 1 — The committed clause inventory is authored; the LLM is never in the CI path

**Decision**: The committed artifact `conformance/inventory/clauses.json` is a
**human-reviewed** list of clause records. An automated (LLM-assisted) proposal step MAY
draft clause boundaries, strength, and testability, but that step is **out-of-band
authoring tooling** — it is *never* invoked by `generate`, `check`, `validate`, `diff`,
`report`, or `certify`. What those CI-facing commands do is **deterministic
normalization + verification over the committed records and the vendored prose**, with no
model and no network. Concretely:

- `clause generate` (misnomer inherited from 020's verb — here it **canonicalizes**, it
  does not invent clauses): reads the committed clause records + the vendored pinned
  prose, recomputes each clause's normalized-substance fingerprint and stable ID from its
  recorded excerpt, verifies the excerpt is present in the pinned document under its
  recorded heading, sorts canonically, and writes the byte-stable artifact. Given an
  already-canonical committed file, output is byte-identical (the determinism contract).
- `clause check` = assert `committed == canonicalize(committed, pinned prose)` — the CLI
  face of the hermetic determinism/provenance test.

**Rationale**: This is the only arrangement that satisfies all of FR-012 (proposal MAY be
automated), FR-013 + SC-004 (no LLM/network in any CI check), FR-023 (hermetic
byte-identical regeneration "operates on the committed, reviewed data, never by
re-invoking a model"), and FR-008/SC-006 (provenance is verifiable offline). The hard,
non-deterministic work (segmenting prose into atomic clauses, judging testability) is
done once by a human (optionally LLM-assisted) and frozen into reviewable data; the
deterministic machine then *guards* that data forever without a model.

**Alternatives considered**: (a) a purely rule-based deterministic prose extractor (RFC
2119 keyword scan + paragraph segmentation) as the `generate` path — rejected: robust
atomic-clause segmentation of real spec prose (multi-requirement paragraphs, algorithm
blocks, I/O contracts in code fences, ambiguity) is exactly what the user input says MAY
need automated/LLM proposal; a naive rule-based splitter would silently mis-segment and
mis-strength, defeating the inventory's purpose. (b) LLM in CI with a fixed seed —
rejected outright by FR-013/SC-004.

## Decision 2 — Identity is substance-anchored (location excluded) so pure moves preserve disposition

**Decision**: A clause's **stable identity** derives from `(document, normalized
substance)` — **not** its heading/location. ID form:
`clu-<doc>-<substance-slug>-<strength>-<hash8>`, where `hash8` = first 8 hex chars of
SHA-256 over `document ‖ normalizedSubstance` (`‖` = fixed separator), `<substance-slug>`
is a bounded slug of the normalized substance's leading tokens (readability only), and
`<strength>` is the strength code. Location (heading/anchor/ordinal) is recorded as
**provenance inside each clause**, never as an ID input.

Consequences, all deterministic:
- **Pure move** (same normalized substance, new heading): ID unchanged → the
  hand-authored classification stays attached → reported as `moved` (location metadata
  changed) and **does not** require re-review or block certification. This is what makes
  FR-020's "report moved headings distinctly" *useful* rather than busywork.
- **Material change** (obligation or strength changes → normalized substance changes):
  new ID → old ID's classification goes V11-stale, new ID unclassified (V12) → blocking
  drift item (FR-021). Strength is folded into the ID via `<strength>` and into the hash
  indirectly (a MUST→SHOULD reword changes the excerpt and thus the substance), so a
  strength change is always a material change.
- **Immaterial change** (whitespace, markdown reflow, description-wording that leaves the
  normalized substance equal): same fingerprint → same ID → not reported as material
  (SC-005).
- **Same obligation stated in several places**: identical normalized substance ⇒ one
  clause unit carrying multiple `locations[]` (natural de-duplication); this is also how
  "several clauses describe one behavior" is represented without collisions.

This **refines** the spec's Assumption wording ("substance-and-location based"): location
is provenance, not an identity input — the refinement is what lets a pure move keep its
disposition (the spec's own goal in FR-020). The spec Assumption is updated in lockstep.

**Rationale**: 020's schema IDs *include* location (`pointer`) and therefore report moves
as remove+add; 021 explicitly requires moves as a distinct, non-destructive category
(FR-020), which is only coherent if identity survives a move. Excluding location from the
hash is the minimal change that achieves it while preserving 020's drift-forcing property
(material change ⇒ new ID).

**Alternatives considered**: (a) location-in-ID like 020 (moves = remove+add) — rejected:
contradicts FR-020's distinct move reporting and forces needless reclassification of
unchanged obligations. (b) fuzzy move-matching by text similarity — rejected:
non-deterministic; identity-by-exact-normalized-substance gives moves for free and stays
byte-stable.

## Decision 3 — Normalized substance: the deterministic core of identity, drift, and immateriality

**Decision**: `normalize_substance(excerpt) -> String` is a fixed, pure function:
lowercase; strip Markdown formatting (emphasis/inline-code/link syntax, list bullets,
block-quote markers); collapse all whitespace runs to a single space; trim; **preserve**
RFC-2119 keywords, identifiers, code-span *contents*, and punctuation that carries meaning
(`/`, `.`, `:`). The clause's `fingerprint` is SHA-256 over this normalized string; the ID
`hash8` is its first 8 hex chars. Material change ⇔ fingerprint change; the diff's
immaterial bucket is exactly "excerpt bytes differ but fingerprint equal."

**Rationale**: Concentrating identity, material-change detection, and
immaterial-change tolerance (SC-005) in one small pure function makes all three
deterministic and unit-testable in isolation, and makes "two humans wrote the excerpt with
different whitespace" a non-event. Mirrors 020's canonical-substance idea, adapted from
JSON values to prose.

**Alternatives considered**: hashing raw excerpt bytes — rejected: whitespace/markdown
reflow would read as material change, violating SC-005. Semantic/embedding similarity —
rejected: non-deterministic, model-dependent.

## Decision 4 — Deterministic strength detection cross-checks the authored label

**Decision**: A pure `detect_strength(excerpt) -> Option<Strength>` maps RFC-2119 keyword
presence to a strength: `MUST`/`MUST NOT`/`REQUIRED`/`SHALL`/`SHALL NOT` → `must`;
`SHOULD`/`SHOULD NOT`/`RECOMMENDED` → `should`; `MAY`/`OPTIONAL` → `may`; none present →
`None`. Strengths `algorithm`, `io-contract`, and `descriptive` are authored labels not
derivable from a single keyword. Validation (V15, Decision 8) cross-checks: a clause
labeled `must`/`should`/`may` MUST contain the corresponding keyword family in its
excerpt; a `descriptive` clause MUST NOT contain an unqualified RFC-2119 mandatory keyword
(that would be a mis-label hiding a requirement, FR-005). `detect_strength` returning
`None` on a clause the author labeled `must` is a V15 violation surfacing a mismatch.

**Rationale**: This turns "normative-strength detection" (FR-006, an FR-027 mandatory
test) into a deterministic, LLM-free property that both the out-of-band proposal aid and
the CI validator share — the label is never taken on faith. Ambiguity (Decision 5) is the
honest `None` outcome that must be resolved by a human.

## Decision 5 — Ambiguity is a testability value that blocks until a human resolves it

**Decision**: `testability` ∈ {`directly-testable`, `indirectly-testable`, `informative`,
`ambiguous`, `not-applicable`}. `ambiguous` means the strength or meaning could not be
confidently determined (hedged language, undefined term, conflated requirement). An
`ambiguous` clause MUST carry a per-clause classification before `certify` passes — a
document-scope default (Decision 7) does **not** clear ambiguity; the validator treats an
unresolved `ambiguous` clause as unclassified (blocking). The proposal aid is forbidden
from promoting `ambiguous` to `must`; only a human edit changes it.

**Rationale**: Implements FR-014 + the fail-closed clarification (any unclassified/ambiguous
clause blocks). Ambiguity is fail-loud, never silently strict, mirroring the registry's
gap semantics.

## Decision 6 — Vendor the pinned prose under `conformance/spec/<pin>/` with a fingerprinted manifest

**Decision**: Commit byte-exact copies of the ratified `docs/specs/` Markdown documents at
`conformance/spec/113500f4/`, with a `manifest.json` recording per document: logical key,
filename, upstream URL at the pinned commit, SHA-256, and a `scope` marker
(`consumer` | `authoring`). The manifest references `rev-spec-113500f4`, keeping
`revisions.json` the single pin authority. Every command verifies each file's SHA-256
before parsing and hard-fails on mismatch (FR-003, V14).

Document set — **all 18 ratified `docs/specs/` documents at `113500f4`** (verified against
the upstream tree; keys derived from filenames, exact fingerprints captured at the vendoring
task). 14 are consumer-scope (per-clause classification) and 4 are authoring-scope
(document-scope not-applicable default permitted):

| key | scope | upstream `docs/specs/…` file |
|---|---|---|
| `reference` | consumer | `devcontainer-reference.md` (lifecycle, resolution, merging, substitution) |
| `json-reference` | consumer | `devcontainerjson-reference.md` (properties reference) |
| `supporting-tools` | consumer | `supporting-tools.md` (CLI behavior reference) |
| `image-metadata` | consumer | `image-metadata.md` (the `devcontainer.metadata` label deacon reads) |
| `lockfile` | consumer | `devcontainer-lockfile.md` (feature lockfile deacon consumes) |
| `devcontainer-id-variable` | consumer | `devcontainer-id-variable.md` (`${devcontainerId}` substitution) |
| `parallel-lifecycle` | consumer | `parallel-lifecycle-script-execution.md` |
| `features-lifecycle-scripts` | consumer | `features-contribute-lifecycle-scripts.md` |
| `features-user-env` | consumer | `features-user-env-variables.md` |
| `feature-dependencies` | consumer | `feature-dependencies.md` (installer dependency resolution) |
| `gpu-host-requirement` | consumer | `gpu-host-requirement.md` (`hostRequirements.gpu`) |
| `declarative-secrets` | consumer | `declarative-secrets.md` |
| `secrets-support` | consumer | `secrets-support.md` |
| `features-legacy-ids` | consumer | `features-legacyIds-deprecated-properties.md` |
| `features` | authoring | `devcontainer-features.md` (Features authoring; consumer install clauses handled per-clause via override, Decision 7) |
| `features-distribution` | authoring | `devcontainer-features-distribution.md` (publishing) |
| `templates` | authoring | `devcontainer-templates.md` (Templates authoring) |
| `templates-distribution` | authoring | `devcontainer-templates-distribution.md` (publishing) |

The `features`/`templates` authoring docs are mixed (authoring model + a consumer
install/apply contract, both in deacon's consumer scope per constitution II). They keep an
`authoring` document-scope default for the bulk, with per-clause `behavior-mapped` overrides
for the consumer install/apply clauses (resolution order, Decision 7); a consumer clause is
never left silently covered by the blanket default — the classifier must override it.

**Rationale**: FR-002/SC-004 require fully offline PR/release testing; vendoring is the
only arrangement where CI never fetches. Fingerprints make "vendored copy ≠ claimed
revision" a blocking error. The `scope` marker drives the document-scope disposition
default (Decision 7). Mirrors 020 Decision 1 exactly, in a sibling directory
(`conformance/spec/` next to `conformance/schemas/`). Network is used once, by the human
vendoring a revision (quickstart.md).

**Alternatives considered**: submodule / fetch-at-test — rejected for the same reasons as
020 Decision 1 (breaks offline clone-and-test). Vendoring only consumer docs — rejected:
FR-001/FR-015 require the full ratified surface, with authoring marked not-applicable, so
that an upstream requirement moving from an authoring doc into consumer scope surfaces as
drift rather than being invisible.

## Decision 7 — Document-scope disposition default keeps the authoring surface tractable

**Decision**: A classification record MAY be **document-scoped** — one record dispositioning
every clause of an `authoring`-scope document as `not-applicable` with a shared rationale
(consumer-only scope, constitution II) — in addition to per-clause records. Resolution
order for a clause: a per-clause record wins; else the document-scope default applies; else
the clause is unclassified (V12, blocking). Two guard rails: (a) a document-scope default is
permitted **only** for documents the manifest marks `authoring` (a consumer document must be
classified clause-by-clause); (b) a clause whose `testability` is `ambiguous` is **never**
covered by a document-scope default — it needs an explicit per-clause decision.

**Rationale**: Fail-closed (Q2) requires *every* clause to carry a disposition before the
gate goes live, including the hundreds of authoring-doc clauses. Per-clause not-applicable
records for entire authoring documents would be pure bookkeeping churn with no reviewer
value. The document-scope default records the honest, uniform decision once while keeping
consumer docs strictly per-clause and never letting ambiguity hide behind a blanket default.

**Alternatives considered**: per-clause not-applicable for authoring docs (020's exactly-one
rule) — rejected: intractable and low-value for whole-document exclusions. Dropping authoring
docs from the inventory — rejected: violates FR-015 and blinds the drift workflow to
authoring→consumer migrations.

## Decision 8 — Reuse V11–V14 generalized to inventory units; add V15 for clause↔source integrity

**Decision**: Generalize 020's violation classes from "constraint unit" to "inventory unit
(schema constraint OR prose clause)" and run the join for both inventories:
- **V11** stale classification — references a unit ID (`cst-`/`clu-`) absent from its
  committed inventory.
- **V12** unclassified — a unit with neither a per-unit classification nor (clauses only)
  a covering document-scope default; or classified more than once. An unresolved
  `ambiguous` clause is V12 by construction (Decision 7).
- **V13** malformed classification — behavior missing / arity rules / id-tail mismatch /
  document-scope default applied to a non-authoring document.
- **V14** provenance — manifest fingerprint mismatch, inventory `revision` ≠ registry pin,
  or committed inventory ≠ canonicalized regeneration (byte-identity).
- **V15 (new)** clause↔source integrity — a clause's `strength` label contradicts its
  excerpt keywords (Decision 4), a `descriptive` clause hides a mandatory keyword, or a
  clause's `excerpt` is not present in the pinned document under its recorded heading
  (provenance-in-source failure).

All are blocking in `validate`; `certify` additionally fails while any unit is unclassified
or any classification is stale — the clause-level analogue of "a gap always blocks."
`not-applicable`/`informative` never block. RULES.md and `validate.rs` stay in lockstep.

**Rationale**: Generalizing the existing classes (rather than minting V16–V19 for clauses)
keeps one enforcement vocabulary across both inventories and one certify gate. V15 is genuinely
new because prose clauses have a source-text-integrity dimension that schema constraints (whose
substance IS the parsed JSON) do not.

## Decision 9 — Diff semantics: match on fingerprint; moves are first-class

**Decision**: `clause diff <old.json> <new.json>` matches units on **normalized-substance
fingerprint** (⇔ the substance-anchored ID). Present-right-only → **new**;
present-left-only → **removed**; ID present in both but its `locations` differ → **moved**
(old and new heading shown); a fingerprint present on neither side that supersedes a removed
one at the same heading is reported as the removal + a new item (a material change is a
remove-of-old-ID + add-of-new-ID, which the review reads as "changed" because they share a
heading). Immaterial differences (excerpt bytes differ, fingerprint equal) are reported in a
separate non-material section. Output is deterministic (sorted by document, then heading, then
ID) in JSON and Markdown, mirroring `report`.

**Rationale**: A fingerprint match key makes moves detectable and deterministic (FR-019/FR-020)
— the key difference from 020's `(document, pointer, kind)` key, which cannot express a move.
Material change surfaces as new/stale IDs, which is exactly what V11+V12 enforce against the
regenerated committed inventory (the diff is advisory; the enforcement is the join).

## Decision 10 — Rollout inside the feature: certify wiring is the last phase (SC-008)

**Decision**: Sequence as 020 did: (1) vendor prose + author the committed clause inventory +
deterministic `generate`/`check` + `diff`, with inventory self-validation (V14/V15) active;
(2) classification records (per-clause for consumer docs, document-scope for authoring docs)
land for 100% of clauses, with V11–V13 active in `validate`; (3) only then does `certify` gain
the "unclassified/stale clause blocks" rule (FR-018/FR-022), the report sections, and any
supersession of hand-written prose source units in `sources/spec.json` whose behavior links
move onto the corresponding `clu-` classifications (FR-026 traceability / no dual bookkeeping). The feature merges as
one CI-gated PR with all three phases complete, so `certify` on main is never red and never
weakened.

**Rationale**: Implements SC-008 and mirrors 020 Decision 10 / feature 019's single-PR delivery
shape; every intermediate commit stays green for bisectability.

## Decision 11 — Behavior-mapping evidence rule (inherited from 020 Decision 11)

**Decision**: A consumer clause is disposed, in order: (1) an existing behavior covers it →
`behavior-mapped` (prefer extending a behavior over minting near-duplicates; several clauses
MAY map to one behavior per FR-010); (2) no behavior but deacon has a hermetic/integration test
exercising it → create the behavior with honest three-axis values AND a `case-` record naming
that test, then map; (3) no evidence at all → write the small hermetic test in-feature, or the
honest state is a **gap** (blocks certify, blocks merge). Minting an uncovered behavior or a
decorative waiver to go green is forbidden.

**Rationale**: Keeps SC-008 achievable honestly and preserves the registry's evidence-backed
semantics. Prose clauses map to the SAME behavior records 020's schema constraints do where they
describe one behavior (e.g., a `forwardPorts` typing constraint and the prose sentence defining
`forwardPorts` both map to `bhv-readconfig-…`), realizing "several clauses → one behavior."

## Resolved unknowns summary

No NEEDS CLARIFICATION markers remain. The three `/speckit.clarify` answers (document scope →
Decision 6; fail-closed triage → Decisions 5/7/8/10; excerpt + fingerprint as distinct fields →
Decisions 2/3) are fully absorbed. The only spec refinement is the identity wording (Decision 2),
updated in the spec Assumptions in lockstep. No deferrals are created; there is no "## Deferred
Work" carried into tasks.md from this research.
