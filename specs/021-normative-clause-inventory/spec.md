# Feature Specification: Normative Clause Inventory

**Feature Branch**: `021-normative-clause-inventory`
**Created**: 2026-07-24
**Status**: Draft
**Input**: User description: "Create a feature specification for maintaining a complete, reviewable inventory of normative and behaviorally relevant clauses from the containers.dev specification pinned by the conformance registry. Cover the full pinned specification rather than only sections known to be implemented. Authoring-only material must be recorded and explicitly marked not applicable under Deacon's consumer-only scope. Break source material into atomic assertions that can be mapped to normalized conformance behaviors. Record stable provenance including source revision, document path, heading or anchor, clause identity, normative strength, scope, and whether the assertion is directly testable, indirectly testable, informative, ambiguous, or not applicable. Distinguish MUST, SHOULD, MAY, algorithm definitions, input/output contracts, and descriptive guidance. Do not infer that every paragraph is a requirement. When one paragraph contains several independent requirements, represent them separately. When several clauses describe one behavior, permit them to map to the same behavior. The workflow may propose extracted clauses automatically, but the committed normative inventory must be human-reviewable and must not require an LLM during CI. Ambiguous source language must be surfaced for classification rather than silently converted into a strict requirement. Provide deterministic drift detection between specification revisions. Report new, removed, moved, and materially changed clauses. New or changed clauses remain unclassified until reviewed and must block strict certification when applicable to Deacon. Acceptance tests are mandatory for stable identities, multi-requirement paragraphs, normative-strength detection, moved headings, changed source text, ambiguous clauses, explicit authoring-scope exclusions, deterministic output, and traceability into the conformance registry."

## Overview

The conformance registry records deacon's conformance against the upstream
containers.dev specification, pinned at a specific revision (`rev-spec-113500f4`).
Feature 020 already produced a complete, machine-extracted inventory of the pinned JSON
**schemas**. But the specification is more than its schemas: the bulk of the normative
surface — lifecycle ordering, configuration-resolution algorithms, merge rules, variable
substitution, feature installation semantics, input/output contracts — lives in the
specification's **prose** documents. Today those prose requirements enter the registry
only as a handful of hand-written source units, written from memory of the sections
deacon happens to implement. There is no systematic record of which normative clauses of
the pinned prose exist, which deacon covers, which are consciously out of scope, and
which have simply never been read — nor any way to detect when an upstream prose revision
adds, removes, moves, or rewords a requirement.

This feature closes that blind spot for the prose surface, mirroring what 020 did for
schemas. It maintains a **complete, reviewable, clause-level inventory** of the pinned
specification prose — the *whole* pinned specification, not only implemented sections —
breaking each document into atomic normative assertions with stable provenance and an
explicit classification. Authoring-only material (feature authoring, template authoring,
publishing, editor integration) is inventoried in full and explicitly marked
not-applicable under deacon's consumer-only scope rather than dropped. The workflow may
use automated (including LLM-assisted) extraction to *propose* clauses, but the committed
inventory is a human-reviewed, version-controlled artifact, and every CI check that
consumes it runs deterministically and offline with no LLM in the loop. Ambiguous source
language is surfaced for a human to classify rather than silently promoted into a strict
requirement. A deterministic drift report between revisions surfaces new, removed, moved,
and materially changed clauses as reviewable items instead of letting old dispositions
silently carry forward.

## Clarifications

### Session 2026-07-24

- Q: Which document set constitutes "the full pinned specification" to inventory? → A:
  All ratified `docs/specs/` documents — consumer-facing AND authoring
  (features/templates/distribution) — with authoring documents inventoried in full but
  classified not-applicable under consumer-only scope. Draft `proposals/` documents are
  out of scope (they churn and would generate perpetual drift noise); if a proposal is
  later ratified into `docs/specs/` at a new pin, it enters scope through the normal
  drift workflow.
- Q: How does a brand-new, untriaged clause behave at the certification gate before
  anyone has judged its consumer-relevance? → A: Fail-closed — ANY unclassified or
  ambiguous clause blocks certification until a human assigns it an explicit disposition,
  even one that later proves not-applicable. Consumer-relevance is never guessed by the
  extractor; the not-applicable classification is a human decision, and only that human
  decision (or an informative/behavior-mapped one) clears the block.
- Q: How is each clause's source text represented in the committed inventory? → A: Both —
  a verbatim excerpt of the clause text (so the committed artifact and its PR diffs are
  human-readable) AND a normalized substance fingerprint (so material-vs-immaterial change
  is detected deterministically, independent of whitespace/reflow). The excerpt serves
  reviewability; the fingerprint serves drift detection; they are distinct fields.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Build the complete clause inventory from the pinned prose (Priority: P1)

A maintainer runs clause extraction across the full pinned containers.dev specification
prose and receives a complete, stably ordered list of **atomic normative assertions** —
one per independent requirement, algorithm step, or input/output contract the prose
expresses. A paragraph that states several independent requirements yields several
clauses; several sentences describing one behavior may be grouped or cross-referenced to
one behavior. Each clause carries provenance (source revision, document path, heading or
anchor, and a stable clause identity), a detected **normative strength** (MUST / SHOULD /
MAY / algorithm-definition / input-output-contract / descriptive-guidance), and a
proposed **testability class** (directly testable, indirectly testable, informative,
ambiguous, or not-applicable). Automated proposal is permitted, but the emitted artifact
is plain, reviewable, and free of any runtime LLM dependency.

**Why this priority**: Everything else (classification, drift, certification) operates on
the inventory. Without a complete and trustworthy clause list there is nothing to classify
or diff. This story alone delivers value: for the first time the true size and shape of
the pinned *prose* requirement surface is visible and reviewable.

**Independent Test**: Run extraction on the pinned prose and on purpose-built fixture
documents; verify every independent requirement expressed by a fixture appears exactly
once as an atomic clause with correct provenance and detected strength, that a
multi-requirement paragraph splits into the expected clauses, and that two consecutive
runs produce byte-identical output.

**Acceptance Scenarios**:

1. **Given** the pinned specification prose, **When** the maintainer builds the inventory,
   **Then** the output contains one atomic clause for every independent normative
   assertion across the *full* pinned specification (not only implemented sections), each
   tagged with source revision, document path, heading/anchor, a stable clause identity,
   its normative strength, and a proposed testability class.
2. **Given** a paragraph containing several independent requirements, **When** extraction
   runs, **Then** each requirement becomes its own clause; and **Given** several sentences
   describing a single behavior, **Then** they may be grouped or cross-linked to the same
   behavior rather than being force-split.
3. **Given** prose containing MUST, SHOULD, and MAY statements, an algorithm definition,
   an input/output contract, and purely descriptive guidance, **When** extraction runs,
   **Then** each clause's normative strength is detected and recorded, and descriptive
   guidance is NOT recorded as a requirement.
4. **Given** the same pinned inputs, **When** extraction runs twice (or on different
   machines), **Then** the outputs are identical byte-for-byte, including ordering and
   clause identifiers.
5. **Given** normal PR or release testing, **When** the inventory is built or checked,
   **Then** no network access occurs and no LLM is invoked; only pinned local artifacts
   are read by deterministic code.

---

### User Story 2 - Classify every clause under the consumer-only scope (Priority: P2)

A maintainer reviews the inventory and ensures every clause carries exactly one explicit
disposition: mapped to one or more conformance behaviors (a clause may share a behavior
with other clauses, and a behavior may cover several clauses); informative / non-testable
(descriptive guidance, with rationale); not-applicable under deacon's consumer-only scope
(feature authoring, template authoring, publishing, or editor-only material, with
rationale); or **ambiguous — pending human resolution**. An ambiguous or unclassified
clause is never silently treated as a strict requirement; it is surfaced as a review item.
Consumer-runtime requirements enter the certification scope; not-applicable, informative,
and (once resolved) ambiguous clauses remain visible in the inventory rather than
disappearing.

**Why this priority**: The inventory only prevents blind spots if unclassified and
ambiguous clauses are impossible to ignore. This story turns the raw extraction into an
auditable coverage claim the release certification gate can consume, and guarantees that
soft or unclear source language is decided by a human, not by the extractor.

**Independent Test**: Take a generated inventory, classify a subset, leave one clause
ambiguous and one consumer-runtime clause unclassified, and verify validation reports
precisely those two as blocking; verify a clause explicitly classified not-applicable
(authoring scope) does not block and remains listed with its rationale.

**Acceptance Scenarios**:

1. **Given** a generated inventory, **When** validation runs, **Then** every clause is
   either mapped to at least one conformance behavior, classified informative /
   non-testable, or classified not-applicable — and any clause with none of these
   (including any clause marked ambiguous) is reported as a blocking review item.
2. **Given** a clause drawn from an authoring-only document (feature authoring, template
   authoring, publishing, or editor integration), **When** it is classified not-applicable
   under consumer-only scope, **Then** it remains present in the inventory and reports with
   its classification and rationale — it does not disappear, and it does not block
   certification.
3. **Given** a clause whose source language is ambiguous (e.g., soft "should generally",
   an undefined term, or a conflated requirement), **When** extraction proposes it,
   **Then** it is surfaced as ambiguous and MUST be resolved by a human into a concrete
   disposition — it is never auto-promoted into a strict MUST.
4. **Given** several clauses that describe one behavior and a consumer-runtime clause
   mapped to a conformance behavior, **When** the certification gate evaluates coverage,
   **Then** the mapped clauses count toward the certified surface exactly as other
   registry-covered behaviors do, and a clause can be traced to its behavior and back.

---

### User Story 3 - Detect and review upstream prose drift (Priority: P3)

When the pinned specification revision is updated, a maintainer generates a deterministic
diff between the old and new revisions that reports every **new, removed, moved, and
materially changed** clause. A heading that moves (reordered or re-nested) is reported as a
move; source text that changes materially (a requirement's substance or strength) is
reported as changed; formatting-only or immaterial wording changes are not reported as
material. Every new or materially changed clause appears as an **unclassified drift item**
requiring review, and no clause inherits a disposition merely because a similarly worded
clause existed before.

**Why this priority**: Drift handling is only exercised when the pin moves, which is
infrequent — but when it happens, this is what prevents silent conformance rot as the
upstream prose evolves.

**Independent Test**: Prepare two fixture revisions differing by one added clause, one
removed clause, one clause under a moved heading, and one materially reworded clause;
verify the diff reports exactly those with the correct change kinds (new / removed / moved
/ changed), that a formatting-only edit is reported as immaterial, and that the
added/changed clauses surface as unclassified drift items that block certification until
reviewed.

**Acceptance Scenarios**:

1. **Given** two specification revisions, **When** the diff is generated, **Then** it
   deterministically lists every new, removed, moved, and materially changed clause with
   the correct change kind — and immaterial changes (whitespace, reflowing, description
   wording that does not change substance) are not reported as material.
2. **Given** an upstream revision that moves a heading or re-nests a section, **When** the
   diff is reviewed, **Then** clauses whose substance is unchanged but whose location
   moved are reported as **moved** (with old and new location), distinct from new and
   changed clauses.
3. **Given** an upstream revision that reshapes or rewords a requirement, **When** the diff
   is reviewed, **Then** the changed clause appears unclassified — it never inherits the
   prior clause's disposition based on wording or heading similarity alone.
4. **Given** unreviewed drift items applicable to deacon, **When** the release
   certification gate runs, **Then** it blocks until every such drift item has been
   reviewed and classified.

---

### User Story 4 - Trust the committed inventory offline, without an LLM (Priority: P4)

A reviewer inspects the committed clause inventory and classification records directly in a
pull request. Automated (possibly LLM-assisted) proposal may have produced the initial
draft, but nothing in CI — validation, drift detection, coverage, or certification —
invokes an LLM or the network; all of it runs deterministically from the committed, pinned,
human-reviewed artifacts. A reviewer can read a clause, follow its provenance to the exact
document and heading in the pinned revision, and trace it into the conformance registry's
behaviors.

**Why this priority**: The base machinery (P1–P3) is only trustworthy if its CI-facing
consumption is reproducible and human-auditable. This story makes the "reviewable, LLM-free,
offline" guarantee explicit and testable, but it rides on the artifacts the earlier stories
produce.

**Independent Test**: Run the full validate/diff/certify path in a network-isolated
environment with no model available and confirm it completes deterministically; pick any
committed clause and confirm its provenance resolves to a real location in the pinned prose
and to a registry behavior (or an explicit not-applicable/informative rationale).

**Acceptance Scenarios**:

1. **Given** the committed inventory and classifications, **When** CI validation, drift
   detection, and certification run, **Then** they complete deterministically with zero
   network requests and zero LLM invocations.
2. **Given** any clause in the committed inventory, **When** a reviewer follows its
   recorded provenance, **Then** it resolves to the exact pinned source revision, document
   path, and heading/anchor, and (for consumer-runtime clauses) to the conformance
   behavior(s) it maps to.
3. **Given** an automatically proposed extraction, **When** it is committed, **Then** the
   committed artifact is plain, diff-reviewable data — a human's review and sign-off is a
   precondition of committing, and the proposal tooling is never a runtime dependency of
   any CI check.

---

### Edge Cases

- A single sentence that states two independent obligations (e.g., "the tool MUST do X and
  MUST NOT do Y") must yield two clauses, not one conflated clause.
- A requirement expressed across multiple sentences or a bulleted list (one lead-in plus
  several sub-items) must be represented so that each independently testable obligation is
  its own clause while the shared context is preserved.
- Soft or hedged language ("should generally", "is expected to", "typically") must be
  surfaced as ambiguous for human classification, never auto-detected as a strict MUST.
- A clause that mixes a normative obligation with descriptive rationale in the same
  paragraph must separate the obligation (recorded with its strength) from the rationale
  (recorded as informative), not fold the rationale into the requirement.
- Requirements that appear only inside a code block, table, or example (e.g., an
  input/output contract shown as a sample) must still be inventoried where they express a
  testable contract.
- A heading that both moves AND has its clause text materially changed between revisions
  must be represented unambiguously (see Assumptions) rather than being reported only as a
  move or only as a change.
- Authoring-only documents (feature/template authoring, publishing/distribution, editor
  integration) must be inventoried in full and marked not-applicable — they must not be
  skipped at extraction time on scope grounds.
- A pinned prose document that is missing, unreadable, or fails integrity verification at
  generation time must produce an explicit error, never a partial inventory.
- A clause whose normative strength cannot be confidently detected must be surfaced as
  ambiguous rather than defaulted to a specific strength.

## Requirements *(mandatory)*

### Functional Requirements

**Inputs and pinning**

- **FR-001**: The inventory MUST be built exclusively from specification prose pinned by
  the conformance registry's revision records (`rev-spec-113500f4`), and MUST cover **all
  ratified `docs/specs/` documents** — every consumer-facing AND authoring document
  (feature authoring, template authoring, distribution) — not only the sections deacon
  currently implements. Draft `proposals/` documents are out of scope at a given pin; a
  proposal that is ratified into `docs/specs/` at a later pin enters scope via the drift
  workflow (FR-019).
- **FR-002**: Pinned prose documents MUST be stored locally under version control so that
  normal PR and release testing operates without any network access. The system MUST never
  silently follow a live URL to obtain source text.
- **FR-003**: The correspondence between each pinned local prose document and its upstream
  revision MUST be verifiable (e.g., via a recorded content fingerprint checked at
  generation time), and a mismatch MUST fail generation explicitly.

**Extraction and atomicity**

- **FR-004**: Extraction MUST produce **atomic clauses**: when a paragraph, sentence, or
  list expresses several independent requirements, each MUST become its own clause; the
  system MUST NOT infer that every paragraph is a single requirement.
- **FR-005**: Extraction MUST NOT treat descriptive guidance as a requirement. Purely
  descriptive, explanatory, or rationale text MUST be recorded as informative rather than
  normative.
- **FR-006**: Each clause MUST record a detected **normative strength**, distinguishing at
  least: MUST (mandatory), SHOULD (recommended), MAY (optional), algorithm definition,
  input/output contract, and descriptive guidance. When strength cannot be confidently
  detected, the clause MUST be surfaced as ambiguous rather than defaulted.
- **FR-007**: Each clause MUST record a **testability class**: directly testable,
  indirectly testable, informative, ambiguous, or not-applicable.
- **FR-008**: Each clause MUST record provenance sufficient for a reviewer to locate it
  without external systems: source revision, document path, heading or anchor, and a stable
  clause identity. Each clause MUST carry BOTH a verbatim excerpt of its source text (so the
  committed artifact and its PR diffs are human-readable) AND a normalized substance
  fingerprint (so change detection is deterministic and whitespace/reflow-independent) as
  distinct fields. Reports MUST NOT embed large copies of whole source documents; the
  per-clause excerpt plus fingerprint and provenance is sufficient to identify change.
- **FR-009**: Each clause MUST have a **stable identifier** that survives regeneration from
  unchanged inputs, so that classifications attach durably to clauses.
- **FR-010**: Several clauses that describe one behavior MAY map to the same conformance
  behavior; the model MUST permit many-clauses-to-one-behavior as well as
  one-clause-to-one-behavior.
- **FR-011**: Inventory generation MUST be deterministic: identical pinned inputs produce
  identical output (content, identifiers, ordering) across repeated runs and platforms.

**Automated proposal vs. committed artifact**

- **FR-012**: The extraction workflow MAY use automated (including LLM-assisted) means to
  *propose* clauses and their initial strength/testability classification.
- **FR-013**: The committed normative inventory MUST be a human-reviewable,
  version-controlled artifact. No CI check that consumes the inventory (validation, drift
  detection, coverage, certification) may require an LLM or network access; all such checks
  MUST run deterministically from the committed, pinned artifacts.
- **FR-014**: Ambiguous source language MUST be surfaced for human classification and MUST
  NOT be silently converted into a strict requirement. A clause marked ambiguous is treated
  as unresolved until a human assigns it a concrete disposition.

**Scope and classification**

- **FR-015**: The entire pinned prose surface MUST be inventoried — including clauses that
  apply only to feature authoring, template authoring, publishing/distribution, or editor
  integration. No clause may be dropped at extraction time on scope grounds.
- **FR-016**: Every clause MUST carry exactly one explicit disposition: (a) mapped to one or
  more conformance behaviors, (b) informative / non-testable (with rationale), or (c)
  not-applicable under deacon's consumer-only scope (with rationale). A clause left
  unclassified or marked ambiguous is not a disposition and MUST be reported by validation
  as a blocking review item — regardless of its (as-yet-unjudged) consumer-relevance.
  Consumer-relevance is decided by the human classifier, never guessed by the extractor.
- **FR-017**: Clauses classified not-applicable or informative MUST remain present in the
  inventory and in generated reports with their classification and rationale visible — they
  must not disappear from any output.
- **FR-018**: The release certification gate MUST be fail-closed on triage: it MUST treat
  ANY unclassified, ambiguous, or unreviewed clause the same way it treats an open gap
  (blocking) until a human assigns an explicit disposition — even a disposition that later
  proves not-applicable. Once assigned, not-applicable and informative classifications MUST
  NOT block; behavior-mapped consumer-runtime clauses enter the certified surface.

**Drift and diffing**

- **FR-019**: The system MUST produce a deterministic diff between any two pinned
  specification revisions, classifying each difference as **new, removed, moved, or
  materially changed**. Material change MUST be defined over clause substance — compared via
  the normalized substance fingerprint (FR-008) — not over formatting, whitespace, or
  reflowed prose; a change to the verbatim excerpt that leaves the fingerprint unchanged is
  immaterial.
- **FR-020**: A heading or section that moves (reordered or re-nested) while its clause
  substance is unchanged MUST be reported as **moved** — distinct from new, removed, and
  changed — with both its prior and current location.
- **FR-021**: A clause that is new or materially changed in an upstream revision MUST appear
  as an **unclassified drift item** until a human reviews and classifies it. A disposition
  MUST NOT be inherited by a clause on the basis of wording or heading similarity to a prior
  clause.
- **FR-022**: Any unreviewed drift item MUST block release certification until a human
  reviews and classifies it, consistent with FR-018's fail-closed-on-triage rule —
  relevance is not assumed away before review.

**Storage, lifecycle, and traceability**

- **FR-023**: The generated inventory MUST be a committed, version-controlled artifact. A
  hermetic CI check MUST regenerate it from the pinned artifacts and fail on any byte-level
  difference, so the committed inventory can never drift from what the pinned prose
  expresses. (Where extraction depends on an automated proposal step, the committed artifact
  is the reviewed output; the hermetic check operates on the committed, reviewed data, never
  by re-invoking a model.)
- **FR-024**: Classifications MUST be hand-authored records stored separately from the
  generated inventory, referencing clauses by their stable identifiers. Regenerating the
  inventory MUST NOT modify, move, or delete any classification record; validation joins the
  two sets and reports mismatches.
- **FR-025**: A classification record referencing a clause that no longer exists in the
  current inventory MUST fail validation as stale and MUST be deleted (or re-pointed) in the
  same change that updates the inventory — mirroring the registry's self-invalidating waiver
  semantics.
- **FR-026**: Every consumer-runtime clause MUST be traceable into the conformance registry:
  from a clause to the behavior(s) it maps to, and from a behavior back to the clauses that
  motivate it. This traceability MUST be checkable deterministically and offline.

**Verification**

- **FR-027**: Acceptance tests MUST cover, at minimum: stable clause identities across
  regeneration, multi-requirement paragraph splitting, normative-strength detection
  (MUST/SHOULD/MAY/algorithm/io-contract/descriptive), moved headings, changed source text,
  ambiguous-clause surfacing, explicit authoring-scope not-applicable exclusions,
  deterministic (byte-identical) output, and traceability into the conformance registry —
  using purpose-built fixture documents AND assertions against the pinned baseline prose.
- **FR-028**: The baseline assertions MUST pin observable facts about the real extracted
  inventory (e.g., that specific well-known normative clauses of the pinned specification
  are present with the expected strength and provenance), so regressions in extraction logic
  surface against real inputs, not only fixtures.

### Key Entities

- **Pinned Prose Source**: A specification document included in the inventory — identified by
  its registry revision record, its upstream document path, and a verifiable content
  fingerprint of the local pinned copy. The full pinned specification is in scope; no
  document is excluded on implementation-status grounds.
- **Clause**: The atomic record of one normative or behaviorally relevant assertion —
  carrying a stable identifier, its normative strength (MUST / SHOULD / MAY / algorithm /
  input-output-contract / descriptive), its testability class (directly testable /
  indirectly testable / informative / ambiguous / not-applicable), BOTH a verbatim excerpt
  of its source text (for review) AND a normalized substance fingerprint (for deterministic
  drift), and full provenance (source revision, document path, heading/anchor). A
  multi-requirement paragraph yields several clauses; several clauses may map to one behavior.
- **Disposition**: The explicit classification attached to a clause — mapped to conformance
  behavior(s), informative / non-testable (with rationale), or not-applicable under
  consumer-only scope (with rationale). Exactly one per clause; an unclassified or ambiguous
  clause is a blocking review item (fail-closed on triage). Dispositions are hand-authored records
  stored separately from the generated inventory and joined to it by stable clause
  identifiers, so regeneration can never alter a human decision; a disposition whose clause
  no longer exists is a blocking stale-record violation.
- **Ambiguous Clause**: A clause whose normative strength or meaning could not be confidently
  determined — surfaced for human resolution and never auto-promoted into a strict
  requirement. Treated as unresolved (blocking, fail-closed on triage) until classified.
- **Drift Item**: A clause that is new or materially changed relative to the previously
  reviewed revision and has not yet been classified. Blocks certification until reviewed.
- **Revision Diff**: The deterministic comparison of two pinned prose revisions — the sets of
  new, removed, moved, and materially changed clauses, with the substance and old/new
  location of each change.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the pinned specification's documents are represented in the inventory
  (including authoring-only documents marked not-applicable); an independent spot-audit of at
  least 25 randomly sampled normative clauses finds zero missing or mis-attributed clauses and
  zero descriptive paragraphs mis-recorded as requirements.
- **SC-002**: Regenerating the inventory from unchanged pinned inputs produces byte-identical
  output on every run and on every supported platform (zero nondeterministic differences
  observed across repeated CI runs).
- **SC-003**: After classification, zero clauses exist without an explicit disposition, and
  validation demonstrably reports any artificially introduced unclassified or ambiguous
  clause as a blocking review item — regardless of its judged consumer-relevance.
- **SC-004**: Every CI check that consumes the inventory (validation, drift, coverage,
  certification) performs zero network requests and zero LLM invocations, verifiable by running
  in a network-isolated environment with no model available.
- **SC-005**: In drift fixtures, 100% of new, removed, moved, and materially changed clauses are
  reported with the correct change kind, and zero immaterial changes (formatting, whitespace,
  reflowed or reworded-but-equivalent prose) are reported as material.
- **SC-006**: A reviewer can trace any inventory entry to its exact source revision, document
  path, and heading/anchor — and any consumer-runtime clause to its conformance behavior(s) and
  back — using only committed data, without opening external systems or following live URLs.
- **SC-007**: A new or materially changed upstream clause introduced by a revision bump surfaces
  as an unclassified drift item in 100% of drift-fixture cases, and never appears with an
  inherited disposition.
- **SC-008**: The release certification gate remains passing on the main branch at every point
  during the feature's rollout — the gate is wired to consume the prose inventory only after
  every clause carries an explicit disposition (authoring/editor clauses marked
  not-applicable, informative clauses marked as such, consumer clauses behavior-mapped), and
  is never loosened to achieve this.

## Assumptions

- **Pinned prose is vendored**: The pinned specification documents will be committed to the
  repository (as the registry's other pinned data already is), which is what makes offline,
  LLM-free PR/release testing possible. The existing revision record (`rev-spec-113500f4`)
  remains the single authority for which revision is pinned.
- **Automated proposal is an authoring aid, not a runtime dependency**: An LLM-assisted or
  otherwise automated step MAY draft the clause extraction and initial classification, but its
  output is committed only after human review. No CI-facing check re-invokes it; the hermetic
  regeneration check operates on committed, reviewed data. This is how "the workflow may propose
  clauses automatically" coexists with "CI must not require an LLM".
- **Clause identity is substance-anchored; location is recorded provenance, not an identity
  input**: A stable identifier derives from what the clause obliges (its normalized substance)
  plus the document — deliberately excluding the heading/location — so that a materially changed
  clause receives a new identity (and surfaces as drift) while a clause that merely moves keeps
  its identity, and therefore its disposition, without re-review. The clause still records its
  location for provenance; that location just does not participate in the identity hash. (This
  refines an earlier "substance-and-location" phrasing; see research Decision 2.)
- **Moves preserve identity and disposition; they are reported for visibility, not re-review**:
  When a clause's substance is unchanged but its heading/location moves, the diff reports it as
  **moved** (old and new location) and its existing disposition carries over unchanged — a move is
  never a blocking drift item. A clause that both moves and materially changes is reported as a
  **change** (old identity removed/stale, new identity unclassified and blocking), because its
  substance — the basis for identity — changed. Movement of unchanged substance is the only case
  reported as a move. This keeps the diff deterministic while honoring the explicit requirement to
  report moved headings distinctly.
- **Ambiguity defaults to blocking, not to strictness**: Consistent with the registry's rule that
  admissions of missing knowledge (gaps) block release certification, any ambiguous or unclassified
  clause blocks until a human resolves it (fail-closed on triage) — the extractor never resolves
  ambiguity by promoting a clause to a strict MUST, nor by guessing it is not-applicable.
- **Not-applicable means recorded, not invisible**: Authoring-only and editor-only clauses are
  inventoried and classified not-applicable under consumer-only scope; they stay visible in the
  inventory and reports and never enter the certification scope.
- **This feature extends existing machinery**: Classification lives in the existing conformance
  registry model (behaviors, coverage, the validate/report/certify commands), extended to consume
  the prose clause inventory alongside feature 020's schema constraint inventory — it does not
  create a parallel conformance system.
- **Full initial classification is in scope; the gate goes live last**: This feature is not
  complete until every clause of the pinned prose carries a disposition (fail-closed on triage
  means authoring/editor clauses must be explicitly marked not-applicable, not merely left out). The
  certification-gate wiring (FR-018/FR-022) is enabled only after that initial classification is
  complete, so the release gate on the main branch never turns red during build-out and is never
  weakened to compensate.

## Dependencies

- The conformance registry (feature 019) — revision records, behavior records, validation,
  reporting, and the certification gate — is the substrate this feature extends.
- The schema constraint inventory (feature 020) — this feature is its prose companion and reuses
  the same disposition/classification/drift patterns and the same certify gate, covering the prose
  surface the schemas do not express.
- The upstream specification revision currently pinned (`rev-spec-113500f4`); updating the pin is
  the event that exercises the drift workflow.
