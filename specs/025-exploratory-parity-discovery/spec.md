# Feature Specification: Exploratory Parity Discovery

**Feature Branch**: `025-exploratory-parity-discovery`
**Created**: 2026-07-27
**Status**: Draft
**Input**: User description: "Create a feature specification for discovering parity behaviors that curated deterministic cases did not anticipate, using constrained generation, shrinking, metamorphic assertions, and pinned real-world workspaces."

## Why This Feature Exists

The conformance record now answers three questions well and one question not at all.

It knows **what it claims** (behaviors with three-axis dispositions), **whether each claim
is backed by evidence** (cases, waivers, gaps), and — since the coverage model — **which
declared scenario combinations remain uncovered**. Every one of those answers is computed
over a denominator that a human wrote down.

That is the limit. A behavior nobody imagined is not uncovered; it is **absent from the
denominator**, so every report is green about it. The coverage model made the *known* hole
countable. It did nothing about the *unknown* one, and by construction it cannot: enumerating
combinations of dimensions a person chose can only ever redistribute that person's
imagination.

Two forces make that unknown hole real rather than theoretical:

- **The input space is adversarial and open.** Configurations arrive from templates,
  generators, editors, and hand-editing. They carry unknown keys, nulls, empty collections,
  wrong types, conflicting sources, cyclic `extends`, exotic substitution nesting, and
  lifecycle shapes in every representation the schema permits. Curated fixtures encode the
  shapes a maintainer thought of on the day they wrote them.
- **Expected output is often impractical to state.** For a merged configuration document
  over an `extends` chain with substitution, nobody can write the expected bytes by hand.
  Today that is handled by comparing against the pinned reference — which works only for
  inputs somebody thought to compare.

This feature adds the complementary discipline: **generate inputs nobody curated, compare
them, and turn each surviving difference into a reviewable candidate.** It is deliberately a
*discovery* mechanism, not a *gate*. Everything it finds enters the existing deterministic
record only through human review, with a stable behavior identity and a disposition — the
same door every other claim uses. Discovery that could silently widen a tolerance or rewrite
a reference snapshot would not add knowledge; it would launder unknowns into green.

## Clarifications

### Session 2026-07-27

- Q: Where does the findings queue live, and does it participate in certification? → A: In its **own namespace outside the registry** — a sibling of the registry rather than a member of it. The registry loader never reads it and `certify` never takes it as input. Placing it inside the registry would repeat the scenario-dimension mistake of 024: a new record kind added to a loaded collection silently changes the certification denominator, which here would let an unreviewed, stochastically-discovered finding block a release.
- Q: What composes the normalized signature that serves as the deduplication key? → A: **Channel + observable path + difference kind + a normalized value-shape class** (type-changed, present/absent, ordering-changed), with concrete values excluded. Structure alone merges genuinely different defects at the same path; including concrete values makes every generated value its own finding and defeats deduplication entirely.
- Q: What is the seed corpus for mutation in the hermetic generation lane? → A: **Committed conformance fixtures only.** The real-world corpus is a separate canary input consumed exclusively by the network-backed lane and is never a mutation seed — seeding from it would make generation itself network-dependent and non-reproducible from the recorded seed and pinned input set alone.
- Q: What bounds findings-queue growth when a campaign surfaces many differences? → A: A **per-campaign admission cap** on newly admitted distinct signatures, with the suppressed count reported explicitly. An unbounded queue lets one generator defect destroy reviewability; a cap that *fails* the campaign would make discovery gate on its own output. Suppression is always visible — never a silent truncation.
- Q: How is the end-to-end pipeline proven? → A: By a **deliberately injected known difference** that must traverse generation, comparison, minimization, candidate emission, classification, and review-only promotion — the same evidence-source injection discipline that proves observable channels can fail. A real discovered-and-promoted behavior is reported when it occurs but is not an acceptance condition, because it depends on the two implementations actually disagreeing where the generator reaches.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Find a difference nobody curated (Priority: P1)

A maintainer wants to know whether deacon and the pinned reference disagree on inputs no one
has written a case for. They start a discovery campaign with a recorded seed against a pinned
input set. The campaign generates valid and near-valid configurations from the pinned schema
surface and grammar rules, applies controlled mutations to known-valid configurations, runs
both implementations over each candidate, and reports every normalized difference it
observes.

**Why this priority**: This is the entire premise. Without generated inputs reaching past the
parser and a differential comparison over them, none of the other stories has an input.
Delivered alone, it already answers a question the project cannot answer today: *does
anything differ outside what we curated?*

**Independent Test**: Run a campaign with a fixed seed against the pinned input set on a
machine with the verified reference available, and confirm it produces a finding set, that the
candidates exercise stages beyond document parsing, and that re-running the same seed produces
the same candidates and the same findings.

**Acceptance Scenarios**:

1. **Given** a recorded seed and a pinned input set, **When** a campaign is run twice,
   **Then** both runs generate the identical ordered sequence of candidate inputs and report
   the identical finding set.
2. **Given** a campaign run, **When** its generated candidates are inspected, **Then** the
   proportion that fail at the document-syntax stage is below the declared trivial-failure
   ceiling, and the remainder reach configuration resolution or a later stage.
3. **Given** a mutation catalogue covering unknown fields, wrong types, nulls, empty values,
   conflicting configuration sources, invalid Feature identifiers, `extends` cycles,
   substitution edge cases, lifecycle shapes, Compose combinations, and ordering changes,
   **When** a campaign completes, **Then** each mutation category was applied at least once
   and its application count is reported.
4. **Given** the reference implementation is missing or is not the pinned version, **When** a
   differential campaign is started, **Then** it fails loudly naming the cause and reports no
   findings, rather than skipping silently or comparing against an unverified reference.
5. **Given** a campaign budget, **When** the budget is exhausted, **Then** the campaign stops,
   records how much of the planned space it covered, and reports the findings gathered so far
   as a complete, self-describing partial run.

---

### User Story 2 - Minimize the difference and hand over a reviewable candidate (Priority: P1)

A raw differential failure on a generated configuration is nearly unusable: the input is large
and mostly irrelevant, and the difference may be one field deep in a merged document. The
maintainer needs the system to reduce the input while the difference still reproduces, then
emit a single reviewable candidate containing the minimal fixture, the invocation context, the
raw evidence from both sides, the normalized difference, the reference provenance, and a
suggested mapping to a behavior.

**Why this priority**: A finding that cannot be reduced and explained will not be triaged, so
discovery volume converts to reviewer fatigue rather than to knowledge. Minimization is what
makes a finding cheap enough to act on and stable enough to deduplicate.

**Independent Test**: Take a known difference on a large generated input, run minimization, and
confirm the result is smaller, still reproduces the same normalized difference signature, is
minimal with respect to the declared reduction steps, and is packaged as a complete candidate.

**Acceptance Scenarios**:

1. **Given** a finding on a large generated input, **When** minimization runs, **Then** the
   reduced input still produces the same normalized difference signature as the original.
2. **Given** a minimized input, **When** any single further reduction step from the declared
   catalogue is applied, **Then** the difference no longer reproduces — the result is minimal
   with respect to that catalogue.
3. **Given** a minimization run, **When** it is repeated from the same finding and seed,
   **Then** it yields the identical minimal input.
4. **Given** a minimization budget is exhausted before a minimum is reached, **When** the result
   is emitted, **Then** the best reduction found is emitted and explicitly marked as
   not-minimal, rather than being silently presented as minimal or discarded.
5. **Given** a completed candidate, **When** it is inspected, **Then** it contains the minimal
   fixture, the operations and arguments used, the raw evidence from both sides preserved
   separately from the normalized form, the normalized difference, the reference provenance,
   and a suggested behavior mapping (an existing behavior identity or an explicit "no existing
   behavior matches").
6. **Given** a reduction step that changes the difference into a *different* signature, **When**
   minimization evaluates that step, **Then** it rejects the step for the finding under
   reduction and reports the new signature as a separate candidate finding rather than losing
   it.

---

### User Story 3 - Keep the deterministic lane hermetic (Priority: P1)

The pull-request lane's value is that it is fast, hermetic, and truthful. Discovery is the
opposite: it is slow, stochastic across seeds, and — for the real-world corpus — network-backed.
A maintainer must be able to trust that a green pull-request run means what it meant before
this feature existed, and that no discovery activity can reach into it.

**Why this priority**: This is the highest-severity failure mode of the whole feature. A
discovery campaign leaking into the deterministic lane would make pull-request results
non-reproducible and network-dependent — destroying a property that took three prior features
to establish. It is also the smallest story, so there is no reason to defer it.

**Independent Test**: Run the deterministic lane with the network unavailable and confirm it
passes, selects no discovery program, and performs no fetch; then confirm the discovery lanes
are reachable only on a schedule or by explicit invocation.

**Acceptance Scenarios**:

1. **Given** the hermetic deterministic lane, **When** it runs with no network access,
   **Then** it passes and no discovery or corpus-fetch activity is selected.
2. **Given** the lane definitions, **When** they are checked structurally, **Then** every
   discovery program is selected by a scheduled or explicitly-invoked lane and by no
   pull-request lane, and a mismatch fails that structural check.
3. **Given** a discovery campaign reports findings, **When** any pull-request lane runs,
   **Then** its result is unaffected — discovery outcomes never determine a pull-request
   verdict.
4. **Given** a discovery campaign fails or times out, **When** the outcome is reported, **Then**
   the failure is visible in the discovery lane and does not block a release or a pull request.
5. **Given** a discovery lane, **When** it is invoked explicitly, **Then** it accepts a seed and
   a budget and records both in its output.

---

### User Story 4 - Classify and deduplicate what was found (Priority: P2)

Campaigns produce repeats: the same underlying difference surfaces from many generated inputs,
and the same difference reappears on every scheduled run until it is resolved. The maintainer
needs each finding placed into one of a fixed set of causes, and needs repeats collapsed so the
triage queue reflects distinct problems rather than campaign volume.

**Why this priority**: Without classification and deduplication the queue grows without bound
and the nightly report becomes noise that nobody reads — at which point the discovery machinery
is worse than absent, because it appears to be watching.

**Independent Test**: Run two campaigns with different seeds over inputs known to trigger the
same underlying difference, and confirm the queue holds one finding with two witnesses, carrying
one classification.

**Acceptance Scenarios**:

1. **Given** a finding, **When** it is triaged, **Then** it carries exactly one classification
   from the closed set: deacon regression, reference quirk, specification ambiguity,
   unsupported behavior, normalizer defect, or fixture defect.
2. **Given** two findings from different campaigns whose normalized signatures are equal,
   **When** they enter the queue, **Then** they are recorded as one finding with two witnesses,
   not two findings.
3. **Given** two findings with different normalized signatures that map to the same behavior
   identity, **When** they enter the queue, **Then** they remain distinct findings and are
   reported as grouped under that behavior.
4. **Given** a finding classified as a normalizer defect or a fixture defect, **When**
   promotion is attempted, **Then** it is rejected — those classifications describe a defect in
   the discovery or comparison machinery, not a behavior of either implementation, and must be
   fixed rather than recorded.
5. **Given** a finding whose difference stops reproducing on a later campaign, **When** the
   queue is refreshed, **Then** it is reported as no-longer-reproducing rather than silently
   dropped.
6. **Given** an untriaged finding, **When** the queue is reported, **Then** it appears in an
   explicit unclassified bucket whose count is visible, so that "not yet looked at" is never
   indistinguishable from "nothing found".
7. **Given** a campaign that observes more distinct signatures than its admission cap, **When**
   it completes, **Then** it admits at most the cap, reports the suppressed count, and still
   succeeds — the cap bounds reviewer load without turning discovery into a gate on its own
   output.

---

### User Story 5 - Promote a finding only through review (Priority: P2)

A finding worth keeping must become an ordinary deterministic case, indistinguishable from a
hand-authored one: linked to a stable behavior identity, dispositioned against the standard and
the reference, covered by an executable case with a committed minimal fixture. A finding worth
tolerating must become an explicit, scoped, expiring waiver. Neither may happen automatically.

**Why this priority**: This is the guardrail that makes the rest safe to run. Automatic
promotion would let a stochastic process author the record it is supposed to be tested against,
and automatic tolerance would let a difference disappear by being observed.

**Independent Test**: Attempt to have the discovery machinery write into the deterministic
record, and confirm it cannot; then promote a finding by hand and confirm the result validates
as an ordinary case.

**Acceptance Scenarios**:

1. **Given** any discovery run, **When** it completes, **Then** it has written nothing into the
   deterministic record — no behavior, no case, no waiver, no allowed difference, and no
   reference snapshot — and a structural check enforces that no discovery program has such a
   write path.
2. **Given** a reviewed finding, **When** it is promoted, **Then** the change carries a stable
   behavior identity (an existing behavior or a newly authored one with all three axes), a
   disposition, an executable case, and the minimal fixture, and the full validation of the
   record passes on that change.
3. **Given** a promotion that omits a behavior identity or a disposition, **When** validation
   runs, **Then** it fails naming what is missing.
4. **Given** a promoted case, **When** it is executed by the ordinary deterministic runner,
   **Then** it runs like any other case with no discovery-specific machinery involved.
5. **Given** a promoted finding, **When** the queue is reported, **Then** the finding is marked
   promoted and names the case that now carries it, so the same difference is not rediscovered
   and re-triaged forever.
6. **Given** a finding a reviewer decides to tolerate, **When** it is recorded, **Then** it
   becomes a scoped waiver with a rationale and an expiry that self-invalidates when the
   difference stops reproducing — never a blanket allowed difference.
7. **Given** a known difference injected at the evidence source, **When** the pipeline runs,
   **Then** it surfaces, minimizes, produces a candidate, is classified, and is promotable; and
   **When** an injected difference fails to surface, **Then** the run fails loudly as a pipeline
   defect rather than reporting a clean campaign.
8. **Given** a queue holding unreviewed findings, **When** the certification gate runs, **Then**
   its result is identical to a run with an empty queue — the queue is not among its inputs.

---

### User Story 6 - Assert what cannot be written down (Priority: P2)

For many inputs there is no practical way to state the expected output — a merged document
over an `extends` chain with substitution is the standard example. The maintainer needs
assertions of the form "this transformation of the input must (or must not) change the
output", each justified by the specification rather than by intuition.

**Why this priority**: Metamorphic assertions are what let discovery run where the pinned
reference is unavailable, ambiguous, or itself suspect — and they catch a class of defect the
differential cannot see at all, namely deacon and the reference being *consistently* wrong
together about invariance.

**Independent Test**: Apply each declared transformation to a corpus of known-valid
configurations and confirm the declared relation holds; then deliberately break one relation
and confirm the failure is reported and names the relation and the transformation.

**Acceptance Scenarios**:

1. **Given** a known-valid configuration and a transformation declared *irrelevant*, **When**
   the transformation is applied, **Then** the normalized result is unchanged — covering at
   minimum insignificant formatting, comments and trailing commas, and key ordering within
   unordered maps.
2. **Given** a workspace relocated to a different absolute path, **When** the same operation is
   run in both locations, **Then** the normalized results are equal modulo the declared path
   tokenization, and any residual difference is reported.
3. **Given** two representations the specification declares equivalent — including the
   permitted lifecycle command shapes — **When** each is resolved, **Then** the normalized
   results are equal.
4. **Given** a transformation declared *significant* (for example reordering where declaration
   order is normative), **When** it is applied, **Then** the result MUST change, and a failure
   to change is reported as a finding.
5. **Given** any declared relation, **When** the relation set is validated, **Then** each
   relation names its specification ground — a normative clause or a recorded behavior — and a
   relation with no ground fails validation.
6. **Given** a metamorphic failure, **When** the candidate is emitted, **Then** it names the
   relation, the transformation applied, both inputs, and both normalized outputs.

---

### User Story 7 - Watch the real ecosystem (Priority: P3)

Generated inputs explore the space the schema permits; real repositories exercise the space
people actually write, which is differently shaped. The maintainer needs a corpus of pinned
real-world workspaces run as an ecological canary, with provenance recorded so that a finding
can always be traced to an exact upstream state.

**Why this priority**: Highest ecological validity, lowest velocity — the corpus changes only
when someone re-pins it, so it finds fewer new things over time than generation does. It is
also the only story that requires network access, which is why it is last and isolated.

**Independent Test**: Run the corpus canary in the network-backed lane and confirm every entry
resolves to an immutable commit, that provenance is recorded for each, and that an entry naming
a mutable reference is rejected.

**Acceptance Scenarios**:

1. **Given** the corpus manifest, **When** it is validated, **Then** every entry names a
   repository and an immutable commit identifier, and an entry naming a branch, a tag, or a
   floating reference is rejected fail-loud.
2. **Given** a corpus run, **When** it completes, **Then** each entry's recorded provenance
   identifies the repository, the commit, the path within it, and a content digest of the
   materialized workspace.
3. **Given** a fetched entry whose content digest disagrees with the recorded one, **When** the
   run proceeds, **Then** it fails loudly for that entry naming the mismatch, rather than
   comparing against unexpected content.
4. **Given** a corpus entry that is unreachable, **When** the run completes, **Then** the entry
   is reported unreachable and distinguished from an entry that ran and produced no finding.
5. **Given** a corpus finding, **When** it is emitted, **Then** it enters the same
   minimization, classification, deduplication, and review-only promotion pipeline as a
   generated finding, and its candidate names its upstream provenance.
6. **Given** the generation lane, **When** its inputs are inspected, **Then** no corpus entry
   appears among its mutation seeds — the corpus is a comparison input in the network-backed
   lane only, so generation stays reproducible without network access.

---

### Edge Cases

- **A generated input is invalid in a way the harness cannot express.** The candidate is a
  fixture defect, not a behavior difference; it is classified as such, is not promotable, and
  drives a generator fix.
- **Both implementations reject the input, with different messages.** Whether this is a
  difference at all depends on the declared comparison scope. Message text is not compared;
  outcome and diagnostic classification are. A difference in outcome is a finding; a difference
  only in wording is not.
- **The difference is caused by the comparison machinery.** A missing or over-broad
  normalization rule can manufacture a difference or hide one. Such a finding is classified as
  a normalizer defect and is never promoted as a behavior; resolving it changes the
  normalization definition, which invalidates affected recorded evidence.
- **Minimization crosses into a different difference.** Reduction must preserve the signature;
  a step that changes it is rejected for the current finding and the new signature is captured
  as its own finding.
- **Minimization cannot reduce at all.** The original input is emitted, marked not-minimal, with
  the reason.
- **The same normalized signature arises from genuinely different causes.** Signature-based
  deduplication would merge them. The merge is reversible: witnesses are retained per finding,
  and a reviewer can split a finding, which must not resurrect the merged duplicate.
- **A finding stops reproducing between discovery and review.** It is reported as
  no-longer-reproducing with the run that last observed it, and is not silently deleted — the
  disappearance is itself information.
- **The pinned reference or schema pin advances.** Every recorded finding is bound to the pin it
  was found under. On a pin change, findings are re-evaluated against the new pin rather than
  carried forward as still-true.
- **A campaign generates an input that is destructive or unbounded to execute** (for example a
  configuration that would start an unpinned image or run an unbounded command). Execution is
  bounded and constrained; a candidate that cannot be executed safely within the declared
  constraints is discarded and counted, not run.
- **The corpus upstream deletes or force-pushes over a pinned commit.** The entry becomes
  unreachable and is reported as such; provenance already recorded is not rewritten.
- **A scheduled campaign finds nothing.** The run reports zero findings *and* the volume it
  covered, so "nothing found" is distinguishable from "nothing ran".

## Requirements *(mandatory)*

### Functional Requirements

#### Reproducibility and pinning

- **FR-001**: Every discovery run MUST record a seed, and re-running with that seed and the
  same pinned input set MUST reproduce the identical ordered sequence of generated candidates.
- **FR-002**: A run's **pinned input set** MUST be recorded and MUST include, at minimum: the
  schema surface pin, the normative prose pin, the reference implementation version, the
  normalization definition version, the generator grammar version, the mutation catalogue
  version, and the **generator version** — the identity of the pseudorandom stream and the order
  of the reduction catalogue, both of which determine output and neither of which is a grammar or
  a mutation.
- **FR-003**: A run MUST fail loudly if any element of its pinned input set is missing, or is
  present at a version other than the pinned one. It MUST NOT proceed against an unverified
  reference and MUST NOT skip silently.
- **FR-004**: All discovery outputs MUST be byte-stable given the same seed and pinned input
  set: no timestamps, absolute paths, or machine-specific values in compared content.
- **FR-005**: A run MUST record its budget and, on exhaustion, MUST report the portion of the
  planned space covered rather than presenting a truncated run as complete.

#### Constrained generation

- **FR-006**: The generator MUST produce configurations derived from the pinned schema surface
  and the grammar rules relevant to configuration resolution, spanning valid and near-valid
  inputs.
- **FR-007**: Generation MUST respect semantic constraints sufficiently that candidates reach
  meaningful resolution stages. The proportion of a campaign's candidates that fail at the
  document-syntax stage MUST stay below a declared ceiling, and that proportion MUST be
  reported for every run.
- **FR-008**: The generator MUST support **controlled mutation** of known-valid configurations,
  with a declared catalogue covering at minimum: unknown fields, wrong types, nulls, empty
  values, conflicting configuration sources, invalid Feature identifiers, `extends` cycles,
  substitution edge cases, lifecycle representation shapes, Compose combinations, and ordering
  changes.
- **FR-008a**: The seed corpus for mutation MUST be the committed deterministic fixtures only.
  The real-world corpus MUST NOT be a mutation seed source, so that generation remains fully
  reproducible from the recorded seed and pinned input set without network access.
- **FR-009**: Every mutation operator MUST be individually identifiable in a candidate's
  provenance, so a finding names which operators produced it.
- **FR-010**: A run MUST report per-category application counts, and MUST report a category
  that was never successfully applied as an explicit generation deficiency rather than
  omitting it.
- **FR-011**: Generated candidates MUST be executed under bounded resources with a per-candidate
  timeout. A candidate that cannot be executed within the declared safety and resource
  constraints MUST be discarded and counted, never executed.
- **FR-012**: Generation MUST NOT emit candidates that reference unpinned image inputs when the
  candidate is destined for a container-backed comparison.

#### Differential comparison

- **FR-013**: A differential comparison MUST run both deacon and the verified pinned reference
  over the same candidate and compare the declared observable channels.
- **FR-014**: Raw evidence from both sides MUST be preserved separately from its normalized
  form.
- **FR-015**: Comparison MUST reuse the single existing normalization definition. Discovery MUST
  NOT introduce a second, parallel, or discovery-only normalization path.
- **FR-016**: Comparison MUST relate outcomes and structured content, never diagnostic message
  wording.
- **FR-017**: A difference already covered by an existing recorded case, waiver, or allowed
  difference MUST be reported as already-characterized and MUST NOT enter the triage queue as
  new.
- **FR-018**: Discovery MUST NOT consume, extend, or author an entry in the allowed-difference
  mechanism to suppress what it finds. That mechanism records reviewed tolerances; a discovery
  program writing to it would let a difference disappear by being observed.

#### Minimization

- **FR-019**: On a difference, the system MUST attempt to reduce the input while preserving the
  finding's **normalized signature**.
- **FR-020**: Minimization MUST be deterministic: the same finding and seed MUST yield the same
  reduced input.
- **FR-021**: A reduced input MUST be reported as minimal only when no single step from the
  declared reduction catalogue further reduces it while preserving the signature.
- **FR-022**: Minimization MUST be bounded; on budget exhaustion the best reduction found MUST
  be emitted and explicitly marked not-minimal with the reason.
- **FR-023**: A reduction step that changes the signature MUST be rejected for the finding under
  reduction, and the resulting new signature MUST be captured as a separate candidate finding.

#### The reviewable candidate

- **FR-024**: Each candidate MUST contain: the minimal fixture, the operations and arguments
  that produced the difference, the raw evidence from both sides, the normalized difference,
  the reference provenance, and a suggested behavior mapping.
- **FR-025**: The suggested behavior mapping MUST either name an existing behavior identity or
  state explicitly that no existing behavior matches. It MUST NOT invent a behavior identity.
- **FR-026**: A candidate MUST record the full pinned input set under which it was found and the
  seed and campaign that found it.
- **FR-027**: A candidate MUST be self-contained: reproducing it MUST require only the candidate
  and the pinned input set it names.

#### Classification and deduplication

- **FR-028**: Every triaged finding MUST carry exactly one classification from the closed set:
  deacon regression, reference quirk, specification ambiguity, unsupported behavior, normalizer
  defect, fixture defect.
- **FR-029**: An untriaged finding MUST appear in an explicit unclassified bucket with a visible
  count.
- **FR-030**: Findings MUST be deduplicated by normalized signature; equal signatures collapse
  to one finding carrying multiple witnesses.
- **FR-030a**: The normalized signature MUST be composed of exactly: the observable channel, the
  observable path within that channel, the kind of difference, and a normalized value-shape
  class (such as type-changed, present-versus-absent, or ordering-changed). Concrete observed
  values MUST NOT contribute to the signature.
- **FR-031**: Distinct signatures MUST remain distinct findings even when they map to the same
  behavior identity; they MAY be reported grouped under that behavior.
- **FR-032**: Witnesses MUST be retained per finding so a merge can be reviewed and split, and a
  split finding MUST NOT be re-merged by the deduplication rule that originally merged it.
- **FR-033**: A finding whose difference stops reproducing MUST be reported as
  no-longer-reproducing, naming the run that last observed it, and MUST NOT be silently dropped.
- **FR-034**: Findings MUST persist across runs so that deduplication and triage state survive a
  campaign boundary.
- **FR-034a**: The findings queue MUST live in its own version-controlled namespace, separate
  from the deterministic record. The record's loader MUST NOT read it and the certification gate
  MUST NOT take it as input, so that no finding — reviewed or not — can block a release.
- **FR-034b**: A campaign MUST admit at most a declared maximum number of newly distinct
  signatures to the queue, and MUST report the count of signatures it observed but did not
  admit. Exceeding the cap MUST NOT fail the campaign, and suppression MUST NEVER be silent.
- **FR-035**: Normalizer-defect and fixture-defect findings MUST be non-promotable; they
  describe a defect in the discovery or comparison machinery and MUST be resolved there.

#### Review-only promotion

- **FR-036**: No discovery program MAY write into the deterministic record — behaviors, cases,
  waivers, allowed differences, dispositions, or reference snapshots. A structural check MUST
  enforce the absence of such a write path.
- **FR-037**: Promotion MUST require a stable behavior identity: an existing behavior, or a newly
  authored one carrying all three disposition axes.
- **FR-038**: Promotion MUST require a disposition against both the written standard and the
  observed reference; a promotion lacking either MUST fail validation naming what is missing.
- **FR-039**: Promotion MUST commit the minimal fixture and produce a case executable by the
  ordinary deterministic runner, with no discovery-specific execution machinery.
- **FR-040**: A promoted case MUST satisfy every existing validation rule that applies to a
  hand-authored case, including full scenario-context assignment and the corresponding
  coverage-obligation updates in the same change.
- **FR-041**: A finding a reviewer chooses to tolerate MUST become a scoped waiver with a
  rationale and an expiry, self-invalidating when the difference stops reproducing. A blanket
  or unscoped allowed difference MUST be rejected.
- **FR-042**: A promoted finding MUST be marked promoted in the queue and MUST name the case
  that now carries it, so it is not rediscovered and re-triaged.
- **FR-042a**: The pipeline MUST be provable by injecting a known difference at the evidence
  source and requiring it to traverse generation, comparison, minimization, candidate emission,
  classification, and review-only promotion. An injected difference that fails to surface MUST
  fail loudly as a pipeline defect, distinct from a campaign that legitimately found nothing.

#### Metamorphic assertions

- **FR-043**: The system MUST support metamorphic relations of two kinds: **invariance** (a
  declared-irrelevant transformation MUST NOT change the normalized result) and **sensitivity**
  (a declared-significant transformation MUST change it).
- **FR-044**: The declared relation set MUST include at minimum: insignificant formatting;
  comments and trailing commas; key ordering within unordered maps; controlled workspace path
  relocation; equivalent lifecycle command representations; and `extends` flattening against its
  hand-flattened equivalent.
- **FR-045**: Every relation MUST name its specification ground — a normative clause or a
  recorded behavior. A relation with no ground MUST fail validation.
- **FR-046**: Path relocation MUST compare modulo the declared path tokenization, and MUST
  report any residual difference the tokenization does not account for.
- **FR-047**: A metamorphic failure MUST produce a candidate naming the relation, the
  transformation, both inputs, and both normalized outputs.
- **FR-048**: Metamorphic relations MUST be evaluable against deacon alone, so they remain
  usable where the reference is unavailable, ambiguous, or itself under suspicion.

#### Real-world corpus

- **FR-049**: The real-world corpus manifest MUST record, per entry, a repository, an immutable
  commit identifier, the path within the repository, and a content digest of the materialized
  workspace.
- **FR-050**: An entry naming a branch, a moving tag, or any floating reference MUST be rejected
  fail-loud at validation time.
- **FR-051**: A materialized entry whose content digest disagrees with the recorded digest MUST
  fail loudly for that entry.
- **FR-052**: An unreachable entry MUST be reported as unreachable, distinguished from an entry
  that ran and produced no finding.
- **FR-053**: Corpus content MUST NOT be vendored into this repository; it is fetched on demand
  in a network-backed lane, consistent with the existing decision on third-party content.
- **FR-054**: A corpus finding MUST enter the same minimization, candidate, classification,
  deduplication, and review-only promotion pipeline as a generated finding, and its candidate
  MUST name its upstream provenance.
- **FR-054a**: The corpus MUST be consumed only as a direct comparison input in the
  network-backed lane. It MUST NOT feed the generator as a mutation seed (FR-008a).

#### Lane isolation

- **FR-055**: The hermetic deterministic pull-request lane MUST NOT perform network access, fetch
  the corpus, run a generation campaign, or select any discovery program.
- **FR-056**: Discovery MUST be reachable only from a scheduled lane or an explicitly invoked
  lane, and the explicit invocation MUST accept a seed and a budget and record both. The
  real-world corpus canary MUST have a scheduled cadence of its own; leaving it invocation-only
  would make it a canary nobody hears.
- **FR-057**: A structural check MUST enforce lane selection: every discovery program is selected
  by a discovery lane and by no pull-request lane, and a mismatch MUST fail that check.
- **FR-058**: Discovery outcomes MUST NOT determine any pull-request or release verdict. A
  discovery program's exit status MUST reflect whether the campaign ran, never what it found.
- **FR-059**: Discovery MUST NOT be a shipped consumer capability; it is development-only tooling
  and MUST NOT appear in the consumer command surface.
- **FR-060**: The container-backed portion of discovery MUST be separately selectable from the
  configuration-resolution portion, so a campaign can run where containers are unavailable.

#### Reporting

- **FR-061**: Every run MUST produce a report stating the seed, the pinned input set, the budget
  and how much of it was used, candidate volume, per-mutation-category counts, trivial-failure
  proportion, findings by classification, the unclassified count, and the count of distinct
  signatures observed but not admitted under the per-campaign cap.
- **FR-062**: A zero-finding run MUST report the volume it covered, so "nothing found" is
  distinguishable from "nothing ran".
- **FR-063**: Reports MUST be deterministic and free of timestamps and absolute paths, and MUST
  be written to a location that is not part of the version-controlled record.

### Key Entities

- **Campaign** — one discovery run. Holds the seed, the pinned input set, the lane, the budget,
  the generation profile, and the resulting findings.
- **Pinned input set** — the complete set of versioned inputs that determine a campaign's
  output: schema pin, prose pin, reference version, normalization version, grammar version,
  mutation catalogue version.
- **Generation profile** — which grammar rules, mutation operators, and target operations a
  campaign draws on; the knob that trades breadth against depth within a budget.
- **Mutation operator** — one named, individually attributable transformation of a known-valid
  configuration, belonging to one declared category.
- **Candidate input** — a generated or mutated configuration together with the operations and
  arguments to run over it, plus its generation provenance.
- **Finding** — one distinct observed difference. Carries a normalized signature, one or more
  witnesses, raw and normalized evidence, a classification, a triage state, and the pins under
  which it was observed.
- **Normalized signature** — the stable identity of a difference: the observable channel, the
  observable path within it, the kind of difference, and a normalized value-shape class, with
  concrete observed values excluded. It is the deduplication key and the invariant minimization
  must preserve.
- **Witness** — one concrete input that exhibits a finding, with the campaign and seed that
  produced it.
- **Reviewable candidate** — the reviewer-facing package for a finding: minimal fixture,
  context, raw evidence, normalized difference, reference provenance, suggested behavior
  mapping.
- **Metamorphic relation** — a named transformation plus its declared effect (invariance or
  sensitivity) plus its specification ground.
- **Corpus entry** — one pinned real-world workspace: repository, immutable commit, path,
  content digest.
- **Findings queue** — the persistent, reviewable collection of findings across campaigns,
  carrying triage and promotion state. It lives in its own version-controlled namespace beside
  the deterministic record, is never loaded by that record, and is never an input to the
  certification gate.
- **Promotion** — the human-reviewed change that converts a finding into a behavior identity,
  a disposition, and either an executable case or a scoped expiring waiver.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Re-running a recorded seed against its pinned input set reproduces the identical
  candidate sequence and the identical finding set, verified over three consecutive runs, with
  zero differences.
- **SC-002**: In every campaign report, at most 10% of generated candidates fail at the
  document-syntax stage; the remaining 90% or more reach configuration resolution or a later
  stage.
- **SC-003**: Every declared mutation category is applied at least once in a full-budget
  campaign, and any category with zero successful applications is reported as a named
  generation deficiency.
- **SC-004**: Minimization reduces a finding's input by at least 80% (median, measured in
  input size) while preserving its signature, and every finding reported as minimal survives an
  independent check that no single further reduction step preserves the signature.
- **SC-005**: 100% of emitted candidates contain all six required parts — minimal fixture,
  context, raw evidence, normalized difference, reference provenance, suggested behavior
  mapping — and each is independently reproducible from the candidate plus its named pins.
- **SC-006**: Repeating a campaign with an unchanged seed and unchanged pins adds zero new
  findings to the queue.
- **SC-007**: 100% of findings in the queue carry exactly one classification or appear in the
  visible unclassified bucket; no finding is in neither state and none is in both.
- **SC-008**: Zero discovery-authored records exist in the deterministic record: a structural
  check confirms no discovery program can write a behavior, case, waiver, allowed difference,
  disposition, or reference snapshot, and it fails if such a path is introduced.
- **SC-009**: 100% of promoted findings pass the full existing record validation on the
  promoting change, including behavior identity, both disposition axes, scenario context, and
  coverage-obligation updates.
- **SC-010**: 100% of declared metamorphic relations name a specification ground; a relation
  without one fails validation.
- **SC-011**: Deliberately breaking each declared metamorphic relation causes exactly that
  relation to fail and be named — zero relations are inert.
- **SC-012**: 100% of real-world corpus entries resolve to an immutable commit; zero entries
  resolve a branch, moving tag, or floating reference, enforced fail-loud.
- **SC-013**: The hermetic deterministic pull-request lane completes with zero network requests
  and selects zero discovery programs, verified with the network unavailable.
- **SC-014**: No discovery outcome — finding, failure, or timeout — changes any pull-request or
  release verdict, verified by a discovery failure alongside a passing pull-request run.
- **SC-015**: The scheduled discovery campaign completes within 30 minutes, with a per-candidate
  timeout of 60 seconds for configuration-resolution comparison and 5 minutes for
  container-backed comparison.
- **SC-016**: A deliberately injected known difference traverses the full pipeline — generation,
  comparison, minimization, candidate emission, classification, and review-only promotion — and
  an injected difference that fails to surface fails loudly as a pipeline defect. Real
  discovered-and-promoted behaviors are reported when they occur but are not an acceptance
  condition, because they depend on the implementations actually disagreeing where the
  generator reaches.
- **SC-018**: The findings queue is never read by the deterministic record's loader and never
  influences the certification gate, verified by a queue holding unreviewed findings alongside a
  certification run whose result is unchanged by them.
- **SC-019**: Every campaign that exceeds its admission cap reports a non-zero suppressed count;
  zero campaigns truncate the queue silently.
- **SC-017**: A reviewer can go from a report entry to a locally reproducing minimal case using
  only the candidate and the pins it names, in under 10 minutes and with no access to the
  machine that ran the campaign.

## Assumptions

1. **The comparison surface is tiered.** The high-volume tier compares configuration resolution
   and merged-configuration results, which are fast and need no containers; a smaller,
   separately selectable tier drives container-backed operations. This keeps a campaign
   affordable while still reaching Compose combinations and lifecycle shapes.
2. **The differential oracle is the existing pinned reference**, verified at its exact pinned
   version, exactly as the current live comparison does. Discovery introduces no second oracle.
3. **Comparison, normalization, and evidence capture are reused, not reimplemented.** Discovery
   is a new *input source* and a new *triage pipeline* on top of the existing comparison
   machinery; a parallel normalization path would be a defect, not a feature.
4. **The findings queue is version-controlled and hand-triaged, in its own namespace.**
   Deduplication across campaigns and "already triaged" state cannot exist without persistence,
   and the review requirement means a human edits it. It sits beside the deterministic record
   rather than inside it, so neither the record's loader nor the certification gate can see it.
   Machine-generated run reports remain outside version control entirely.
5. **Discovery reports are advisory.** They gate nothing. Only the existing certification gate
   blocks a release, and it does so on the deterministic record, which discovery cannot write.
6. **Two scheduled cadences, not one.** The hermetic generation campaign runs **nightly**,
   alongside the existing nightly live comparison. The real-world corpus canary runs **weekly**
   in the network-backed lane — a canary that runs only when someone remembers to invoke it is
   not a canary. The deep campaign and the container-backed tier remain explicitly invoked with
   a seed and a larger budget.
7. **Classification is a human judgement**, assisted but not decided by the tooling. The tooling
   may suggest a classification; it may not assign one unreviewed.
8. **A pin advance invalidates findings rather than migrating them.** Findings are claims about
   a specific pinned pair of implementations; on a pin change they are re-evaluated, not
   carried forward.
9. **The existing real-world corpus manifest is the starting point** for the pinned canary; its
   entries are already commit-pinned and already recorded as a non-vendored coverage source. It
   is a comparison input only, never a generation seed.
10. **"Near-valid" means schema-adjacent**: inputs a reasonable author could produce — a wrong
    type, a stray key, an empty collection — not arbitrary byte-level corruption. Random binary
    noise is out of scope because it only exercises the document parser.

## Out of Scope

- **A general-purpose fuzzer** for the container runtime, the registry client, or any network
  protocol. The target is configuration and lifecycle behavior parity.
- **Automatic repair.** Discovery reports differences; it never proposes or applies a code change.
- **Automatic promotion, tolerance, or snapshot refresh**, in any form, under any flag.
- **Vendoring third-party workspace content** into this repository.
- **Comparing diagnostic message wording** between implementations.
- **Feature-authoring surfaces**, which remain permanently outside this project's scope.
- **Making any discovery capability part of the shipped consumer command surface.**

## Dependencies

- The pinned schema surface and the pinned normative prose, and their existing provenance
  checks, as the grammar source for generation.
- The verified pinned reference implementation, for differential comparison.
- The single existing normalization definition and its version, as the basis of the normalized
  signature and of every comparison.
- The existing observable-channel set and observers, as the comparison surface.
- The existing record validation, certification gate, and coverage-obligation model, which every
  promotion must satisfy.
- The existing scheduled live-comparison lane, alongside which the discovery lanes run.
- Container tooling for the container-backed tier, and network access for the corpus canary —
  both confined to non-pull-request lanes.
