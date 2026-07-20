# Feature Specification: Schema Constraint Inventory

**Feature Branch**: `020-schema-constraint-inventory`
**Created**: 2026-07-20
**Status**: Draft
**Input**: User description: "Create a feature specification for reproducibly extracting a constraint-level inventory from the containers.dev JSON schemas pinned by the conformance registry. The inventory must include the complete pinned devContainer base schema and dev container Feature schema. Editor-composite schemas or other external schemas may be included only when pinned explicitly and reported as separate sources. Do not silently follow live URLs. Produce atomic source units for testable constraints… Resolve internal schema composition correctly… Detect cycles and malformed or unresolved references explicitly. Inventory generation must be deterministic and stably ordered. Extract the whole pinned schema surface… Each extracted constraint must map to one or more conformance behaviors or be explicitly classified as non-testable or not applicable. A new or changed upstream constraint must appear as an unclassified drift item until reviewed… Provide a deterministic diff between two schema revisions… Normal PR and release testing must operate from pinned local artifacts without network access. Acceptance tests are mandatory for nested composition, union alternatives, null handling, required properties, additional-property rules, cyclic references, malformed schemas, stable IDs, deterministic output, and source revision drift."

## Overview

The conformance registry already records deacon's conformance against the upstream
containers.dev specification, pinned at a specific revision (`rev-schema-113500f4`).
Today, however, the registry's schema-derived source units are **hand-written and
sparse** (two records exist, covering `features` and `forwardPorts` typing), while the
pinned schemas define hundreds of testable constraints. There is no systematic way to
know which schema constraints deacon's conformance record covers, which are consciously
out of scope, and which have simply never been looked at — nor any way to detect when
an upstream schema revision adds, removes, or changes a constraint.

This feature closes that blind spot: it produces a **complete, reproducible,
constraint-level inventory** of the pinned schemas, requires every constraint to carry
an explicit classification (mapped to conformance behaviors, non-testable, or
not-applicable under deacon's consumer-only scope), and provides a deterministic diff
between schema revisions so upstream changes surface as reviewable drift items instead
of silently inheriting old dispositions.

## Clarifications

### Session 2026-07-20

- Q: Where do human classifications live, given the inventory is machine-regenerated? →
  A: Classifications are hand-authored registry records keyed by stable constraint unit
  IDs, stored separately from the generated inventory; validation joins the two.
  Regeneration never destroys or mutates a human decision.
- Q: Is the inventory a committed artifact or regenerated ephemerally? → A: Committed
  artifact under version control, with a hermetic CI check that regenerates it and
  requires byte-identical output. Drift is reviewable in PR diffs; certification
  consumes the committed artifact offline.
- Q: Must the full initial classification land within this feature? → A: Yes. The
  certification-gate wiring is the last step of the feature, enabled only once the
  entire consumer-runtime surface is classified — the gate on the main branch never
  goes red, and is never weakened to compensate.
- Q: What happens to a classification whose constraint disappears after a revision
  bump? → A: It fails validation as stale and must be deleted in the same change,
  mirroring the registry's self-invalidating waiver semantics.
- Q: What happens to the two existing hand-written schema source units? → A: They are
  superseded — their behavior links migrate to the corresponding generated constraint
  units and the hand-written records are retired in the same change (no dual
  bookkeeping).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Generate the complete constraint inventory (Priority: P1)

A maintainer runs the inventory extraction against the pinned devContainer base schema
and the dev container Feature schema, and receives a complete, stably ordered list of
atomic constraint units — one per testable rule the schemas express (property
existence, requiredness, type, nullability, enum membership, default value, union
alternatives, object/array shape, additional-property rules, references, and
conditional composition). Each unit carries provenance: the source file, the pinned
revision, and the exact location within the schema document.

**Why this priority**: Everything else in this feature (classification, drift
detection, diffing) operates on the inventory. Without a complete and trustworthy
extraction there is nothing to classify or diff. This story alone already delivers
value: it makes the true size and shape of the pinned schema surface visible for the
first time.

**Independent Test**: Run extraction on the pinned schemas and on purpose-built fixture
schemas; verify every constraint expressed by a fixture appears exactly once as an
atomic unit with correct provenance, and that two consecutive runs produce identical
output.

**Acceptance Scenarios**:

1. **Given** the pinned devContainer base schema and Feature schema, **When** the
   maintainer generates the inventory, **Then** the output contains one atomic
   constraint unit for every testable constraint in both schemas, each tagged with its
   source file, pinned revision, and in-document location.
2. **Given** a fixture schema exercising nested composition (allOf/anyOf/oneOf,
   including nesting), union alternatives, null handling, required properties, and
   additional-property rules, **When** extraction runs, **Then** each constraint is
   correctly resolved through the composition and appears as its own unit.
3. **Given** the same pinned inputs, **When** extraction runs twice (or on different
   machines), **Then** the outputs are identical byte-for-byte, including ordering and
   identifiers.
4. **Given** a fixture schema containing a cyclic reference, an unresolvable
   reference, or malformed schema content, **When** extraction runs, **Then** it fails
   with an explicit, cause-naming error — it never silently skips or truncates.
5. **Given** normal PR or release testing, **When** the inventory is generated or
   checked, **Then** no network access occurs; only pinned local artifacts are read.

---

### User Story 2 - Classify every constraint under the consumer-only scope (Priority: P2)

A maintainer reviews the inventory and ensures every constraint unit carries exactly
one explicit disposition: mapped to one or more existing conformance behaviors,
classified non-testable (e.g., purely descriptive metadata), or classified
not-applicable under deacon's consumer-only scope (feature-authoring,
template-authoring, or editor-only constraints). Consumer-runtime constraints enter the
certification scope; not-applicable and non-testable constraints remain visible in the
inventory rather than disappearing.

**Why this priority**: The inventory only prevents blind spots if unclassified
constraints are impossible. This story turns the raw extraction into an auditable
coverage claim that the release certification gate can consume.

**Independent Test**: Take a generated inventory, classify a subset, and verify the
validation step reports precisely the still-unclassified units; verify a
consumer-runtime constraint left unclassified blocks certification while a
not-applicable one does not.

**Acceptance Scenarios**:

1. **Given** a generated inventory, **When** validation runs, **Then** every constraint
   unit is either mapped to at least one conformance behavior, classified
   non-testable, or classified not-applicable — and any unit with none of these is
   reported as a blocking violation.
2. **Given** a constraint that applies only to feature authoring, template authoring,
   or editor integration, **When** it is classified not-applicable, **Then** it remains
   present in the inventory and in reports with its classification and rationale — it
   does not disappear.
3. **Given** a consumer-runtime constraint mapped to a conformance behavior, **When**
   the certification gate evaluates coverage, **Then** that constraint counts toward
   the certified surface exactly as other registry-covered behaviors do.

---

### User Story 3 - Detect and review upstream schema drift (Priority: P3)

When the pinned schema revision is updated, a maintainer generates a deterministic diff
between the old and new revisions showing added, removed, and materially changed
constraints. Every added or materially changed constraint appears as an unclassified
drift item requiring review; no constraint inherits a disposition merely because a
similarly named constraint existed before.

**Why this priority**: Drift handling is only needed when the pin moves, which is
infrequent — but when it happens, this is what prevents silent conformance rot.

**Independent Test**: Prepare two fixture revisions differing by one added, one
removed, and one changed constraint; verify the diff reports exactly those three with
correct change kinds, and that the added/changed ones surface as unclassified drift
items that block certification until reviewed.

**Acceptance Scenarios**:

1. **Given** two schema revisions, **When** the diff is generated, **Then** it
   deterministically lists every added, removed, and materially changed constraint —
   and immaterial changes (e.g., formatting, description wording) are not reported as
   material.
2. **Given** an upstream revision that renames or reshapes a constraint, **When** the
   diff is reviewed, **Then** the new constraint appears unclassified — it never
   inherits the old constraint's disposition based on name similarity alone.
3. **Given** unreviewed drift items, **When** the release certification gate runs,
   **Then** it blocks until every drift item has been reviewed and classified.

---

### User Story 4 - Add an explicitly pinned external schema source (Priority: P4)

A maintainer explicitly pins an additional schema (e.g., an editor-composite schema)
and includes it in the inventory as a separately identified source. Sources that are
not explicitly pinned are never fetched or included; live URLs are never followed
silently.

**Why this priority**: The base and Feature schemas cover the mandatory surface;
additional sources are an optional extension of the same machinery.

**Independent Test**: Pin a small external fixture schema, regenerate the inventory,
and verify its constraints appear attributed to the new source; verify an unpinned
reference to an external location produces an explicit error rather than a fetch.

**Acceptance Scenarios**:

1. **Given** an explicitly pinned additional schema, **When** the inventory is
   generated, **Then** its constraints appear attributed to that distinct source with
   its own revision identity, and reports list it separately.
2. **Given** a schema that references an external document that is not pinned,
   **When** extraction runs, **Then** it fails with an explicit unresolved-reference
   error identifying the reference — it never fetches from the network.

---

### Edge Cases

- A constraint reachable through multiple composition paths (e.g., the same property
  constrained in two `allOf` branches) must yield a well-defined result — either
  distinct units with distinct provenance or one merged unit — chosen once and applied
  deterministically.
- Union alternatives (`oneOf`/`anyOf`/type arrays) where alternatives impose different
  constraints on the same property: each alternative's constraints must be
  distinguishable in the inventory.
- Explicit `null` in type unions versus absence of a property: nullability must be
  captured as its own constraint facet, not conflated with optionality.
- `additionalProperties: false` versus a schema-valued `additionalProperties` versus
  absence: all three must be distinguished.
- Self-referential (recursive) schema structures that are valid (e.g., a schema
  referencing itself for nested values) must be inventoried finitely, while genuinely
  unproductive cycles are reported as errors.
- Constraints expressed only through conditional composition (`if`/`then`/`else`)
  must be captured with their condition context preserved.
- Two revisions where a constraint moves location but is otherwise identical: the diff
  must have a defined, documented answer (see Assumptions) rather than nondeterministic
  matching.
- A pinned artifact that is missing, unreadable, or fails integrity verification at
  generation time must produce an explicit error, never a partial inventory.
- An empty or trivially small schema (fixture) must produce a valid, empty-or-small
  inventory, not an error.

## Requirements *(mandatory)*

### Functional Requirements

**Inputs and pinning**

- **FR-001**: The inventory MUST be generated exclusively from schema artifacts pinned
  by the conformance registry's revision records; the pinned devContainer base schema
  and the dev container Feature schema MUST both be included in full.
- **FR-002**: Pinned schema artifacts MUST be stored locally under version control so
  that normal PR and release testing operates without any network access.
- **FR-003**: Additional schemas (e.g., editor-composite schemas) MAY be included only
  when explicitly pinned as their own source with their own revision identity, and
  MUST be reported as separate sources.
- **FR-004**: The system MUST never silently follow a live URL. Any reference that
  would require fetching an unpinned document MUST fail with an explicit
  unresolved-reference error naming the reference.
- **FR-005**: The correspondence between each pinned local artifact and its upstream
  revision MUST be verifiable (e.g., via a recorded content fingerprint checked at
  generation time), and a mismatch MUST fail generation explicitly.

**Extraction**

- **FR-006**: Extraction MUST produce atomic constraint units covering, at minimum:
  property existence, requiredness, declared types, nullability, enum membership,
  default values, union alternatives, object and array shape rules,
  additional-property rules, references, and conditional composition.
- **FR-007**: Each constraint unit MUST record provenance: the source artifact, the
  pinned revision identifier, and the precise in-document location of the constraint.
  Reports MUST NOT embed large copies of the source documents; provenance plus a
  compact excerpt or fingerprint is sufficient to identify change.
- **FR-008**: Extraction MUST resolve internal schema composition correctly, including
  references and arbitrarily nested `allOf`, `anyOf`, and `oneOf` structures, so that
  constraints reachable only through composition are still inventoried.
- **FR-009**: Extraction MUST detect and explicitly report unproductive reference
  cycles, unresolvable references, and malformed schema content as generation-failing
  errors with cause-specific messages. Partial or truncated inventories MUST never be
  produced.
- **FR-010**: Inventory generation MUST be deterministic: identical pinned inputs
  produce identical output (content, identifiers, and ordering) across repeated runs
  and across platforms.
- **FR-011**: Each constraint unit MUST have a stable identifier that survives
  regeneration from unchanged inputs, so classifications attach durably to units.

**Scope and classification**

- **FR-012**: The entire pinned schema surface MUST be extracted — including
  constraints that apply only to feature authoring, template authoring, or editor
  integration. No constraint may be dropped at extraction time on scope grounds.
- **FR-013**: Every constraint unit MUST carry exactly one explicit disposition:
  (a) mapped to one or more conformance behaviors, (b) non-testable (with rationale),
  or (c) not-applicable under deacon's consumer-only scope (with rationale).
  Registry validation MUST report any unit lacking a disposition as a blocking
  violation.
- **FR-014**: Constraints classified not-applicable or non-testable MUST remain
  present in the inventory and in generated reports with their classification and
  rationale visible — they must not disappear from any output.
- **FR-015**: Consumer-runtime constraints MUST enter the certification scope: the
  release certification gate MUST treat an unclassified or unreviewed consumer-runtime
  constraint the same way it treats an open gap (blocking), while not-applicable and
  non-testable classifications MUST NOT block.

**Drift and diffing**

- **FR-016**: The system MUST produce a deterministic diff between any two pinned
  schema revisions, listing added, removed, and materially changed constraints.
  Material change MUST be defined over constraint substance (types, requiredness,
  enum sets, defaults, shapes, composition), not over formatting or prose description
  wording.
- **FR-017**: A constraint that is new or materially changed in an upstream revision
  MUST appear as an unclassified drift item until a human reviews and classifies it.
  Dispositions MUST attach to constraint identity, and identity MUST be derived from
  constraint substance and location — never inferred from name similarity to a prior
  constraint.
- **FR-018**: Unreviewed drift items MUST block release certification until resolved,
  consistent with FR-015.

**Storage, lifecycle, and migration**

- **FR-019**: The generated inventory MUST be a committed, version-controlled artifact.
  A hermetic CI check MUST regenerate it from the pinned artifacts and fail on any
  byte-level difference, so the committed inventory can never drift from what the
  pinned schemas actually express.
- **FR-020**: Classifications MUST be hand-authored records stored separately from the
  generated inventory, referencing constraint units by their stable identifiers.
  Regenerating the inventory MUST NOT modify, move, or delete any classification
  record; validation joins the two sets and reports mismatches.
- **FR-021**: A classification record referencing a constraint unit that no longer
  exists in the current inventory MUST fail validation as stale and MUST be deleted
  (or re-pointed) in the same change that updates the inventory — mirroring the
  registry's self-invalidating waiver semantics.
- **FR-022**: The existing hand-written schema source units in the conformance
  registry MUST be superseded by this feature: their behavior links migrate to the
  corresponding generated constraint units, and the hand-written records are retired
  in the same change. After migration there is exactly one bookkeeping system for
  schema-derived source units.

**Verification**

- **FR-023**: Acceptance tests MUST cover, at minimum: nested composition resolution,
  union alternatives, null handling, required properties, additional-property rules,
  cyclic references, malformed schemas, stable identifiers, deterministic output, and
  source revision drift — using purpose-built fixture schemas AND assertions against
  the pinned baseline schemas.
- **FR-024**: The baseline assertions MUST pin observable facts about the real
  extracted inventory (e.g., that specific well-known constraints of the devContainer
  base schema are present with expected substance), so that regressions in extraction
  logic surface against real inputs, not only fixtures.

### Key Entities

- **Pinned Schema Source**: A schema artifact included in the inventory — identified
  by its registry revision record, its upstream location, and a verifiable content
  fingerprint of the local pinned copy. The devContainer base schema and Feature
  schema are mandatory sources; others are optional and explicit.
- **Constraint Unit**: The atomic record of one testable constraint — carrying a
  stable identifier, the constraint's kind (requiredness, type, enum, …), its
  substance (the testable rule), and full provenance (source, revision, in-document
  location, condition context where applicable).
- **Disposition**: The explicit classification attached to a constraint unit — mapped
  to conformance behavior(s), non-testable (with rationale), or not-applicable under
  consumer-only scope (with rationale). Exactly one per unit; absence is a blocking
  violation. Dispositions are hand-authored records stored separately from the
  generated inventory and joined to it by stable constraint identifiers, so
  regeneration can never alter a human decision; a disposition whose constraint no
  longer exists is a blocking stale-record violation.
- **Drift Item**: A constraint unit that is new or materially changed relative to the
  previously reviewed revision and has not yet been classified. Blocks certification
  until reviewed.
- **Revision Diff**: The deterministic comparison of two pinned revisions — the sets
  of added, removed, and materially changed constraint units, with the substance of
  each change.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of constraints expressed by the pinned devContainer base schema and
  Feature schema appear in the generated inventory; an independent spot-audit of at
  least 25 randomly sampled schema constraints finds zero missing or mis-attributed
  units.
- **SC-002**: Regenerating the inventory from unchanged pinned inputs produces
  byte-identical output on every run and on every supported platform (zero
  nondeterministic differences observed across repeated CI runs).
- **SC-003**: After classification, zero constraint units exist without an explicit
  disposition, and validation demonstrably reports any artificially introduced
  unclassified unit as a blocking violation.
- **SC-004**: Normal PR and release testing involving the inventory performs zero
  network requests (verifiable by running in a network-isolated environment).
- **SC-005**: In drift fixtures, 100% of added, removed, and materially changed
  constraints are reported by the diff with the correct change kind, and zero
  immaterial changes (formatting, description wording) are reported as material.
- **SC-006**: A reviewer can trace any inventory entry to its exact source location
  and pinned revision using only the entry's recorded provenance, without opening
  external systems or following live URLs.
- **SC-007**: A new upstream constraint introduced in a revision bump surfaces as an
  unclassified drift item in 100% of drift-fixture cases, and never appears with an
  inherited disposition.
- **SC-008**: The release certification gate remains passing on the main branch at
  every point during the feature's rollout — the gate is wired only after the full
  initial classification is complete, and is never loosened to achieve this.

## Assumptions

- **Pinned artifacts are vendored**: The pinned schema documents will be committed to
  the repository (as the registry's other data already is), which is what makes
  offline PR/release testing possible. The existing revision record
  (`rev-schema-113500f4`) remains the single authority for which revision is pinned.
- **Unreviewed drift blocks certification**: The user requires drift items to remain
  unclassified until reviewed; consistent with the registry's existing rule that
  admissions of missing knowledge (gaps) always block release certification, this
  spec treats unreviewed drift items as certification-blocking. Not-applicable and
  non-testable classifications never block.
- **Constraint identity is substance-and-location based**: Stable identifiers derive
  from what the constraint says and where it lives, giving deterministic identity
  across regenerations while guaranteeing that a materially changed constraint gets a
  new identity (and therefore surfaces as drift) rather than inheriting the old
  disposition.
- **A moved-but-identical constraint is reported as removed + added**: When a
  constraint's substance is unchanged but its location moves between revisions, the
  diff reports it as a removal plus an addition (each with provenance) rather than
  attempting fuzzy move-tracking; the reviewer can classify the new item quickly
  because the substance is identical. This keeps the diff fully deterministic.
- **Descriptive metadata is non-testable, not invisible**: Schema keywords that carry
  no testable behavior (titles, descriptions, examples, editor hints) are inventoried
  where they define constraint context but classified non-testable rather than
  polluting the certification scope.
- **This feature extends existing machinery**: Classification lives in the existing
  conformance registry model (behaviors, coverage, the validate/report/certify
  commands), extended to consume the generated inventory — it does not create a
  parallel conformance system. The two existing hand-written schema source units are
  superseded and retired as part of the migration (FR-022).
- **Full initial classification is in scope; the gate goes live last**: This feature
  is not complete until every constraint of the pinned base and Feature schemas
  carries a disposition. The certification-gate wiring (FR-015/FR-018) is enabled
  only after that initial classification is complete, so the release gate on the
  main branch never turns red during the build-out and is never weakened to
  compensate.

## Dependencies

- The conformance registry (feature 019) — revision records, behavior records,
  validation, and the certification gate — is the substrate this feature extends.
- The upstream schema revision currently pinned (`113500f4`); updating the pin is the
  event that exercises the drift workflow.
