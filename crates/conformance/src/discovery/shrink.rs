//! Structural delta-debugging over the parsed configuration document
//! (025-exploratory-parity-discovery, data-model.md § 6, US2).
//!
//! Reduction never happens at the byte or line level: text-level ddmin on JSON produces
//! syntactically broken intermediates that cannot reproduce a signature living past
//! parsing, and each one costs a full oracle invocation to discover (research D5).
//!
//! The reproduction predicate is a **parameter** ([`ReproductionProbe`]), not a call into
//! the oracle, so the reduction *strategy* stays hermetic and unit-testable against a
//! synthetic predicate while the live campaign supplies the real one
//! (`parity_harness::discovery::minimize`). Without that split the shrinker could only be
//! tested by running a campaign.
//!
//! ## The order is a pin, not a style choice
//!
//! The **ordered catalogue** below is one half of `generatorVersion` — the seventh element
//! of every campaign's pinned input set (data-model.md § 4) — so a campaign cannot record
//! its own provenance without it. The order is reproducibility-critical (FR-020 requires
//! the same finding and seed to yield the identical minimal input, and greedy reduction is
//! order-sensitive), which is why the order lives here once rather than being restated in
//! the generator: two statements of one order are two statements that can disagree.
//!
//! ## Why the reduction terminates, and why that is not incidental
//!
//! Every accepted step strictly decreases a [`Complexity`] triple that is bounded below, so
//! the greedy loop cannot cycle. This matters because one catalogue step —
//! `un-apply-mutation` — can make the document *larger*: reversing an operator that emptied
//! a collection restores the collection. A size-only objective would let the shrinker
//! oscillate between emptying and un-applying forever, spending the expensive step (an
//! oracle invocation) on a loop. The triple orders "corruptions still standing" ahead of
//! size precisely so that reversal always counts as progress.
//!
//! ## What `isMinimal` claims, and what it does not
//!
//! [`Reduction::is_minimal`] is `true` only when a **complete pass over all seven steps**
//! produced no proposal that both reduced the document and preserved the signature. That is
//! FR-021's claim exactly: minimal *with respect to the declared catalogue*, a finite and
//! checkable statement — never the unfalsifiable assertion that no smaller input exists.
//! On budget exhaustion the best reduction found is emitted with `is_minimal: false` and a
//! [`Reduction::not_minimal_reason`] (FR-022); a partially reduced input is never silently
//! presented as minimal.

use std::future::Future;

use serde_json::{Map, Value};

use super::mutate::Mutation;
use super::signature::Signature;

/// The seven reduction steps, **in application order** (data-model.md § 6):
/// `drop-optional-key`, `un-apply-mutation`, `empty-collection`,
/// `collapse-extends-level`, `drop-compose-service`, `minimize-scalar`, `drop-feature`.
///
/// Ordered because greedy reduction is order-sensitive: the same finding and seed must
/// yield the identical minimal input (FR-020), and a different order is a different fixed
/// point. Reordering these names is therefore a pin change, not a refactor — which is why
/// the order belongs to `generatorVersion` rather than to `mutationCatalogVersion`, whose
/// subject is the mutation operator set and which would misdescribe it.
///
/// `isMinimal` is true only when all seven have been applied once with no step preserving
/// the signature — which is what makes FR-021's minimality claim finite and checkable
/// rather than an unfalsifiable assertion about all possible smaller inputs.
pub const REDUCTION_STEPS: [&str; 7] = [
    "drop-optional-key",
    "un-apply-mutation",
    "empty-collection",
    "collapse-extends-level",
    "drop-compose-service",
    "minimize-scalar",
    "drop-feature",
];

/// The revision of the catalogue's **order**, bumped whenever a step is added, removed,
/// or moved.
///
/// Distinct from the step names themselves: renaming a step for clarity does not change
/// which minimal input a finding reduces to, but moving one does.
pub const REDUCTION_CATALOGUE_VERSION: u32 = 1;

/// The reduction catalogue's identity, as it appears inside a campaign's
/// `generatorVersion` — the ordered step names plus the order's revision.
///
/// Spelling the order out rather than hashing it keeps the pinned input set readable: a
/// reviewer comparing two campaigns can see *which* step moved, which an opaque digest
/// would hide behind "the generator changed".
pub fn reduction_catalogue_identity() -> String {
    format!(
        "reduce[{}]/v{REDUCTION_CATALOGUE_VERSION}",
        REDUCTION_STEPS.join(",")
    )
}

// ---------------------------------------------------------------------------
// The catalogue, as executable steps
// ---------------------------------------------------------------------------

/// One declared reduction step (data-model.md § 6).
///
/// The enum and [`REDUCTION_STEPS`] are cross-checked by a unit test rather than one being
/// derived from the other: the constant is the *pin* (it is embedded in every recorded
/// campaign's `generatorVersion`) and the enum is the *implementation*, and a silent
/// derivation would let a step be added to the implementation without the pin moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReductionStep {
    /// Remove a root key the grammar does not mark `required`.
    DropOptionalKey,
    /// Reverse one recorded mutation operator, exactly (`mutate::Reversal`).
    UnApplyMutation,
    /// Replace a non-empty array or object with an empty one of the same shape.
    EmptyCollection,
    /// Inline one `extends` level and remove the link.
    CollapseExtendsLevel,
    /// Remove one Compose service the document does not reference through `service`.
    DropComposeService,
    /// Replace a scalar with the schema-minimal value of its own type.
    MinimizeScalar,
    /// Remove one entry from `features`.
    DropFeature,
}

impl ReductionStep {
    /// Every step, **in application order**.
    pub fn all() -> &'static [ReductionStep; 7] {
        &[
            ReductionStep::DropOptionalKey,
            ReductionStep::UnApplyMutation,
            ReductionStep::EmptyCollection,
            ReductionStep::CollapseExtendsLevel,
            ReductionStep::DropComposeService,
            ReductionStep::MinimizeScalar,
            ReductionStep::DropFeature,
        ]
    }

    /// The stable wire spelling, recorded in `Witness::reduction_steps`.
    pub fn name(self) -> &'static str {
        match self {
            ReductionStep::DropOptionalKey => "drop-optional-key",
            ReductionStep::UnApplyMutation => "un-apply-mutation",
            ReductionStep::EmptyCollection => "empty-collection",
            ReductionStep::CollapseExtendsLevel => "collapse-extends-level",
            ReductionStep::DropComposeService => "drop-compose-service",
            ReductionStep::MinimizeScalar => "minimize-scalar",
            ReductionStep::DropFeature => "drop-feature",
        }
    }
}

// ---------------------------------------------------------------------------
// The predicate (the parameter, research D4/D5)
// ---------------------------------------------------------------------------

/// What a probe found when it re-ran a reduced input.
///
/// Three answers rather than a boolean, because FR-023 needs the third: a step that changes
/// the signature must be *rejected for the finding under reduction* **and** the signature it
/// produced instead must be captured as a separate candidate finding. A `bool` predicate
/// would throw that away at the moment it was observed, and the difference would have to be
/// rediscovered by a later campaign — or never.
#[derive(Debug, Clone, PartialEq)]
pub enum Reproduction {
    /// The signature under reduction reproduced, unchanged.
    Preserved,
    /// The input still differs, but not at the signature under reduction. Every signature
    /// observed instead is carried so the caller can admit it separately (FR-023).
    Drifted(Vec<Signature>),
    /// Nothing differed at all: the reduction removed the difference.
    Absent,
}

/// The reproduction predicate — the **parameter** that keeps this module hermetic.
///
/// The live implementation re-runs both CLIs (`parity_harness::discovery::minimize`), which
/// is why the method is async; the hermetic tests here implement it synchronously over a
/// declared rule and never start a process. That is the whole point of research D4/D5:
/// without the split, the reduction *strategy* could only be exercised by running a
/// campaign against the pinned oracle.
pub trait ReproductionProbe {
    /// Whatever the implementation's probe can fail with. A probe failure aborts the
    /// reduction rather than being read as "did not reproduce": an input whose comparison
    /// could not be run has said nothing about whether it reproduces, and treating silence
    /// as a rejection would quietly stop reducing.
    type Error;

    /// Whether `document` still reproduces the signature under reduction.
    fn probe(
        &mut self,
        document: &Value,
    ) -> impl Future<Output = Result<Reproduction, Self::Error>>;
}

// ---------------------------------------------------------------------------
// Input and result
// ---------------------------------------------------------------------------

/// What a reduction starts from.
#[derive(Debug, Clone, Default)]
pub struct ReductionInput {
    /// The document as the campaign observed it.
    pub document: Value,
    /// The mutation operators applied to it, in application order, each carrying its exact
    /// reversal.
    pub mutations: Vec<Mutation>,
    /// The root keys the grammar marks `required` for this document's branch.
    ///
    /// Supplied by the caller rather than re-derived here: the grammar is the authority on
    /// which keys a valid instance must carry (research D1), and a second view of the
    /// pinned surface could disagree with the one generation used. An empty list means
    /// "nothing is protected", which reduces harder rather than less — the predicate still
    /// rejects anything that stops reproducing.
    pub required_keys: Vec<String>,
}

/// A signature a rejected step produced **instead** of the one under reduction (FR-023).
///
/// Carries the document it was seen on, not merely the signature. The reduction rejected
/// that proposal and went elsewhere, so its own final document very likely does not
/// reproduce this signature at all — and a witness naming an input that does not reproduce
/// its own signature is a record nobody can re-examine, which is the one thing a finding
/// must never be.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftedFinding {
    /// The signature observed instead.
    pub signature: Signature,
    /// The rejected proposal it was observed on.
    pub document: Value,
}

/// The outcome of one reduction.
#[derive(Debug, Clone, PartialEq)]
pub struct Reduction {
    /// The reduced document.
    pub document: Value,
    /// The catalogue steps applied, in application order — what
    /// `Witness::reduction_steps` records.
    pub steps: Vec<String>,
    /// **Only** true when a complete pass over all seven steps preserved nothing
    /// (FR-021).
    pub is_minimal: bool,
    /// Why it is not minimal. `Some` exactly when [`is_minimal`](Self::is_minimal) is
    /// `false` (FR-022): a partially reduced input is never presented as minimal, and
    /// never presented as not-minimal without saying why.
    pub not_minimal_reason: Option<String>,
    /// How many probes the reduction spent — the expensive unit, and the one the budget
    /// bounds.
    pub probes: u64,
    /// Signatures a rejected step produced *instead* of the one under reduction (FR-023),
    /// each with the input it was seen on. Deduplicated by signature id, in first-observed
    /// order.
    pub drifted: Vec<DriftedFinding>,
    /// The `mop-` operators still standing on the reduced document.
    pub remaining_mutations: Vec<String>,
    /// Node count of the input document.
    pub original_size: usize,
    /// Node count of the reduced document — the SC-004 measure.
    pub reduced_size: usize,
}

impl Reduction {
    /// The identity reduction: the input unchanged, explicitly **not** minimal, carrying
    /// the reason no reduction was attempted.
    ///
    /// The caller that skips minimization — a re-witness of a finding the queue already
    /// carries, or a campaign whose wall clock has run out — still has to produce a
    /// witness, and FR-022 forbids presenting an unreduced input as minimal every bit as
    /// much as it forbids presenting a partially reduced one that way. Routing the skip
    /// through this constructor means "not minimal, and here is why" is the only shape a
    /// skipped reduction can take.
    pub fn not_attempted(input: &ReductionInput, reason: impl Into<String>) -> Reduction {
        let size = node_count(&input.document);
        Reduction {
            document: input.document.clone(),
            steps: Vec::new(),
            is_minimal: false,
            not_minimal_reason: Some(reason.into()),
            probes: 0,
            drifted: Vec::new(),
            remaining_mutations: input.mutations.iter().map(|m| m.operator.clone()).collect(),
            original_size: size,
            reduced_size: size,
        }
    }

    /// The fraction of the input the reduction removed, in nodes (SC-004's measure).
    ///
    /// `0.0` for an empty input rather than a division by zero: a document with no nodes
    /// was already as small as it can be, and reporting a non-finite fraction would
    /// serialize as bare `null` and never load back.
    pub fn size_reduction_fraction(&self) -> f64 {
        if self.original_size == 0 {
            return 0.0;
        }
        1.0 - (self.reduced_size as f64 / self.original_size as f64)
    }
}

// ---------------------------------------------------------------------------
// The greedy loop
// ---------------------------------------------------------------------------

/// Reduce `input` while preserving the signature `probe` tests for (FR-019 – FR-023).
///
/// `budget` bounds **probes**, not accepted steps: the probe is the expensive unit (the
/// live one runs two CLIs), and a budget counting accepted steps would place no bound at
/// all on a pass that proposes many reductions and accepts none.
///
/// The loop restarts from the first catalogue step after every acceptance. That is what
/// makes the exit condition meaningful: when it returns with `is_minimal: true`, a complete
/// pass over all seven steps has just been made against the returned document with nothing
/// accepted — FR-021's claim, established rather than asserted.
pub async fn reduce<P: ReproductionProbe>(
    input: &ReductionInput,
    budget: u64,
    probe: &mut P,
) -> Result<Reduction, P::Error> {
    let original_size = node_count(&input.document);
    let mut current = input.document.clone();
    let mut mutations = input.mutations.clone();
    let mut steps: Vec<String> = Vec::new();
    let mut drifted: Vec<DriftedFinding> = Vec::new();
    let mut probes: u64 = 0;
    let mut exhausted = false;

    'pass: loop {
        for step in *ReductionStep::all() {
            for proposal in proposals(step, &current, &mutations, input) {
                // Only strictly-reducing proposals are ever probed. The check is pure and
                // free; the probe is neither, and spending an oracle invocation to learn
                // that a non-reduction still reproduces is the budget waste research D5
                // objects to at generation time, relocated to minimization.
                if complexity(&proposal.document, proposal.mutations.len())
                    >= complexity(&current, mutations.len())
                {
                    continue;
                }

                // A proposal byte-identical to the current document cannot change the
                // outcome, so it is accepted without a probe. This is not an optimization
                // in disguise: `un-apply-mutation` reaches it whenever an earlier step
                // already removed everything an operator introduced, and probing it would
                // spend the run's most expensive step to re-learn what is already known.
                if proposal.document == current {
                    mutations = proposal.mutations;
                    steps.push(step.name().to_string());
                    continue 'pass;
                }

                if probes >= budget {
                    exhausted = true;
                    break 'pass;
                }
                probes += 1;

                match probe.probe(&proposal.document).await? {
                    Reproduction::Preserved => {
                        current = proposal.document;
                        mutations = proposal.mutations;
                        steps.push(step.name().to_string());
                        continue 'pass;
                    }
                    Reproduction::Drifted(signatures) => {
                        // FR-023: rejected for THIS finding, and the new signature is
                        // captured rather than discarded. Discarding it would mean the
                        // machinery observed a difference and then deliberately forgot it.
                        //
                        // The rejected proposal is carried alongside, because it — not the
                        // document this reduction eventually settles on — is the input that
                        // reproduces the drifted signature.
                        for signature in signatures {
                            if !drifted.iter().any(|d| d.signature.id == signature.id) {
                                drifted.push(DriftedFinding {
                                    signature,
                                    document: proposal.document.clone(),
                                });
                            }
                        }
                    }
                    Reproduction::Absent => {}
                }
            }
        }
        // A complete pass over all seven steps accepted nothing: minimal with respect to
        // the declared catalogue (FR-021).
        break;
    }

    let not_minimal_reason = exhausted.then(|| {
        format!(
            "the shrink budget of {budget} probe(s) was exhausted after {} accepted step(s); \
             the best reduction found is reported and is NOT minimal (FR-022)",
            steps.len()
        )
    });

    Ok(Reduction {
        reduced_size: node_count(&current),
        document: current,
        steps,
        is_minimal: !exhausted,
        not_minimal_reason,
        probes,
        drifted,
        remaining_mutations: mutations.iter().map(|m| m.operator.clone()).collect(),
        original_size,
    })
}

/// Whether any single catalogue step further reduces `document` while preserving the
/// signature — the independent minimality check SC-004 requires of every input reported as
/// minimal.
///
/// Separate from [`reduce`] on purpose: `reduce` establishes minimality as a *consequence*
/// of how its loop exits, and a claim that rests on control flow is worth re-checking with
/// a function whose only job is to look. Returns the first step that still reduces, or
/// `None` when the input is genuinely minimal.
pub async fn first_further_reduction<P: ReproductionProbe>(
    input: &ReductionInput,
    probe: &mut P,
) -> Result<Option<&'static str>, P::Error> {
    for step in *ReductionStep::all() {
        for proposal in proposals(step, &input.document, &input.mutations, input) {
            if complexity(&proposal.document, proposal.mutations.len())
                >= complexity(&input.document, input.mutations.len())
            {
                continue;
            }
            if proposal.document == input.document {
                return Ok(Some(step.name()));
            }
            if probe.probe(&proposal.document).await? == Reproduction::Preserved {
                return Ok(Some(step.name()));
            }
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Complexity: why every accepted step is progress
// ---------------------------------------------------------------------------

/// The objective the greedy loop strictly decreases: `(corruptions, nodes, non-minimal
/// scalars)`, compared lexicographically.
///
/// Each component exists because exactly one catalogue step reduces it and no other step
/// increases it without reducing an earlier one:
///
/// | Component | Reduced by | Why it must lead |
/// |---|---|---|
/// | mutations still standing | `un-apply-mutation` | reversal can *grow* the document, so size cannot judge it |
/// | node count | the five removal steps | the ordinary notion of "smaller" |
/// | scalars not at their type-minimum | `minimize-scalar` | `5 → 0` and `true → false` change no node count and may grow the text |
///
/// Ordering corruptions first is what forbids an emptying/un-applying oscillation: reversal
/// is always progress, so the loop can never return to a document it has already left.
type Complexity = (usize, usize, usize);

fn complexity(document: &Value, mutations_remaining: usize) -> Complexity {
    (
        mutations_remaining,
        node_count(document),
        non_minimal_scalars(document),
    )
}

/// Every JSON node, counting object keys as nodes of their own.
///
/// Keys count because dropping a key whose value is a scalar must register as a reduction:
/// counting only values would make `{"a": 1}` and `{}` differ by one either way, which is
/// fine, but `{"a": null}` → `{}` would too, and a metric that cannot tell an authored
/// `null` from an omission is the conflation 023 T062 already paid for once.
fn node_count(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(node_count).sum::<usize>(),
        Value::Object(map) => 1 + map.values().map(|v| 1 + node_count(v)).sum::<usize>(),
        _ => 1,
    }
}

/// How many scalar positions hold something other than the minimum of their own type.
fn non_minimal_scalars(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.iter().map(non_minimal_scalars).sum(),
        Value::Object(map) => map.values().map(non_minimal_scalars).sum(),
        other => usize::from(minimal_scalar(other).as_ref() != Some(other)),
    }
}

/// The schema-minimal value of a scalar's **own type**, or `None` when there is none.
///
/// Type-preserving by construction (data-model.md § 6, step 6): a reduction that changed a
/// string into `null` would be a `wrong-type` *mutation* wearing a reduction's name, and the
/// reduced input would no longer be an instance of what the finding was found on.
fn minimal_scalar(value: &Value) -> Option<Value> {
    match value {
        Value::String(_) => Some(Value::String(String::new())),
        Value::Number(_) => Some(Value::from(0)),
        Value::Bool(_) => Some(Value::Bool(false)),
        // `null` is already minimal, and a container is not a scalar.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Proposals
// ---------------------------------------------------------------------------

/// One candidate reduction: the document it would produce and the mutations that would
/// still stand on it.
#[derive(Debug, Clone)]
struct Proposal {
    document: Value,
    mutations: Vec<Mutation>,
}

/// Every proposal `step` makes against `document`, in a **deterministic** order.
///
/// Determinism here is FR-020: the same finding reduces to the same input because the same
/// proposals are offered in the same sequence and the first accepted one wins. Nothing in
/// this module draws from the PRNG — the reduction is seed-*independent*, which is stronger
/// than FR-020 asks for and considerably easier to hold.
fn proposals(
    step: ReductionStep,
    document: &Value,
    mutations: &[Mutation],
    input: &ReductionInput,
) -> Vec<Proposal> {
    let keep = |doc: Value| Proposal {
        document: doc,
        mutations: mutations.to_vec(),
    };
    match step {
        ReductionStep::DropOptionalKey => root_keys(document)
            .into_iter()
            .filter(|key| !input.required_keys.iter().any(|r| r == key))
            .filter_map(|key| remove_root_key(document, &key).map(keep))
            .collect(),

        ReductionStep::UnApplyMutation => (0..mutations.len())
            .map(|index| {
                let mut remaining = mutations.to_vec();
                let removed = remaining.remove(index);
                Proposal {
                    document: removed.reversal.apply(document),
                    mutations: remaining,
                }
            })
            .collect(),

        ReductionStep::EmptyCollection => positions(document)
            .into_iter()
            .filter_map(|path| {
                let emptied = match at(document, &path)? {
                    Value::Array(items) if !items.is_empty() => Value::Array(Vec::new()),
                    Value::Object(map) if !map.is_empty() => Value::Object(Map::new()),
                    _ => return None,
                };
                replace_at(document, &path, emptied).map(keep)
            })
            .collect(),

        ReductionStep::CollapseExtendsLevel => {
            collapse_extends(document).into_iter().map(keep).collect()
        }

        ReductionStep::DropComposeService => drop_compose_service(document)
            .into_iter()
            .map(keep)
            .collect(),

        ReductionStep::MinimizeScalar => positions(document)
            .into_iter()
            .filter_map(|path| {
                let current = at(document, &path)?;
                let minimal = minimal_scalar(current)?;
                if &minimal == current {
                    return None;
                }
                replace_at(document, &path, minimal).map(keep)
            })
            .collect(),

        ReductionStep::DropFeature => feature_ids(document)
            .into_iter()
            .filter_map(|id| remove_feature(document, &id).map(keep))
            .collect(),
    }
}

/// `collapse-extends-level` — inline one `extends` parent and remove the link.
///
/// A chain of two or more loses its **last** link (one level inlined); a chain of one loses
/// the `extends` key entirely. There is no third case: a parent this process cannot read
/// contributes nothing to inline, so removing the link *is* the collapse. Naming that
/// explicitly rather than declining to act keeps the step honest — an `extends` chain is one
/// of the largest contributors to a finding's input, and a step that silently did nothing
/// would leave it standing while `isMinimal` still claimed the catalogue was exhausted.
fn collapse_extends(document: &Value) -> Vec<Value> {
    let Some(extends) = document.get("extends") else {
        return Vec::new();
    };
    match extends {
        Value::Array(items) if items.len() >= 2 => {
            let mut shortened = items.clone();
            shortened.pop();
            replace_at(
                document,
                &[Seg::Key("extends".to_string())],
                Value::Array(shortened),
            )
            .into_iter()
            .chain(remove_root_key(document, "extends"))
            .collect()
        }
        _ => remove_root_key(document, "extends").into_iter().collect(),
    }
}

/// `drop-compose-service` — remove one service the document does not reference.
///
/// Expressed over `runServices`, because that is where a *document* names services: the
/// services themselves are declared in the Compose project beside it, and the candidate's
/// Compose file is fixture scaffolding the campaign writes rather than candidate content.
/// The service named by `service` is never dropped — it is referenced by definition, and a
/// document that lists it under `runServices` and then loses it there has changed which
/// services run, not how many are declared.
fn drop_compose_service(document: &Value) -> Vec<Value> {
    let Some(Value::Array(services)) = document.get("runServices") else {
        return Vec::new();
    };
    let primary = document.get("service");
    (0..services.len())
        .filter(|index| Some(&services[*index]) != primary)
        .filter_map(|index| {
            let mut remaining = services.clone();
            remaining.remove(index);
            replace_at(
                document,
                &[Seg::Key("runServices".to_string())],
                Value::Array(remaining),
            )
        })
        .collect()
}

/// The `features` keys, in the map's own (sorted) order.
fn feature_ids(document: &Value) -> Vec<String> {
    match document.get("features") {
        Some(Value::Object(features)) => features.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

fn remove_feature(document: &Value, id: &str) -> Option<Value> {
    let Some(Value::Object(features)) = document.get("features") else {
        return None;
    };
    let mut remaining = features.clone();
    remaining.remove(id)?;
    replace_at(
        document,
        &[Seg::Key("features".to_string())],
        Value::Object(remaining),
    )
}

fn root_keys(document: &Value) -> Vec<String> {
    match document {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

fn remove_root_key(document: &Value, key: &str) -> Option<Value> {
    let Value::Object(map) = document else {
        return None;
    };
    let mut out = map.clone();
    out.remove(key)?;
    Some(Value::Object(out))
}

// ---------------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------------

/// One segment of a position within a document.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Key(String),
    Index(usize),
}

/// Every position in `document` **except the root**, in pre-order.
///
/// Pre-order puts shallow positions first, so the largest available reduction is offered
/// before its own descendants — which matters because the loop restarts after each
/// acceptance and a subtree emptied at its root never has to have its children visited at
/// all. The root itself is excluded: emptying it would produce `{}` for every finding and
/// reduce two unrelated defects to the same input.
fn positions(document: &Value) -> Vec<Vec<Seg>> {
    let mut out = Vec::new();
    let mut prefix = Vec::new();
    walk(document, &mut prefix, &mut out);
    out
}

fn walk(value: &Value, prefix: &mut Vec<Seg>, out: &mut Vec<Vec<Seg>>) {
    match value {
        Value::Object(map) => {
            for key in map.keys() {
                prefix.push(Seg::Key(key.clone()));
                out.push(prefix.clone());
                prefix.pop();
            }
            for (key, child) in map {
                prefix.push(Seg::Key(key.clone()));
                walk(child, prefix, out);
                prefix.pop();
            }
        }
        Value::Array(items) => {
            for index in 0..items.len() {
                prefix.push(Seg::Index(index));
                out.push(prefix.clone());
                prefix.pop();
            }
            for (index, child) in items.iter().enumerate() {
                prefix.push(Seg::Index(index));
                walk(child, prefix, out);
                prefix.pop();
            }
        }
        _ => {}
    }
}

fn at<'a>(document: &'a Value, path: &[Seg]) -> Option<&'a Value> {
    let mut cursor = document;
    for segment in path {
        cursor = match segment {
            Seg::Key(key) => cursor.as_object()?.get(key)?,
            Seg::Index(index) => cursor.as_array()?.get(*index)?,
        };
    }
    Some(cursor)
}

fn replace_at(document: &Value, path: &[Seg], value: Value) -> Option<Value> {
    let Some((head, rest)) = path.split_first() else {
        return Some(value);
    };
    match (head, document) {
        (Seg::Key(key), Value::Object(map)) => {
            let child = map.get(key)?;
            let replaced = replace_at(child, rest, value)?;
            let mut out = map.clone();
            out.insert(key.clone(), replaced);
            Some(Value::Object(out))
        }
        (Seg::Index(index), Value::Array(items)) => {
            let child = items.get(*index)?;
            let replaced = replace_at(child, rest, value)?;
            let mut out = items.clone();
            out[*index] = replaced;
            Some(Value::Array(out))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::mutate::{self, MutationCategory};
    use crate::discovery::rng::Prng;
    use crate::discovery::signature::{Divergence, DivergenceKind};
    use serde_json::json;

    // -----------------------------------------------------------------------
    // The catalogue's identity (US1)
    // -----------------------------------------------------------------------

    #[test]
    fn the_catalogue_declares_the_seven_steps_in_data_model_order() {
        assert_eq!(
            REDUCTION_STEPS,
            [
                "drop-optional-key",
                "un-apply-mutation",
                "empty-collection",
                "collapse-extends-level",
                "drop-compose-service",
                "minimize-scalar",
                "drop-feature",
            ],
            "the ORDER is part of `generatorVersion` (FR-020): reordering these changes \
             which minimal input every recorded finding reduces to, so it is a reviewed \
             pin change rather than a refactor"
        );
    }

    #[test]
    fn step_names_are_unique() {
        let mut names = REDUCTION_STEPS.to_vec();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two reduction steps share a name");
    }

    #[test]
    fn the_identity_names_the_order_rather_than_hiding_it_behind_a_digest() {
        let identity = reduction_catalogue_identity();
        assert!(identity.starts_with("reduce[drop-optional-key,"));
        assert!(identity.ends_with("]/v1"));
        for step in REDUCTION_STEPS {
            assert!(identity.contains(step), "{step} missing from the identity");
        }
    }

    #[test]
    fn the_executable_steps_are_the_pinned_steps_in_the_pinned_order() {
        // The pin and the implementation are two statements of one order, cross-checked
        // rather than derived: deriving the constant from the enum would let a step be
        // added to the implementation without `generatorVersion` moving, and every
        // recorded campaign would then name a catalogue it did not run.
        let executable: Vec<&str> = ReductionStep::all().iter().map(|s| s.name()).collect();
        assert_eq!(executable, REDUCTION_STEPS.to_vec());
    }

    // -----------------------------------------------------------------------
    // Driving the future without an async runtime
    // -----------------------------------------------------------------------

    /// Poll a future to completion on the current thread.
    ///
    /// This crate deliberately declares **no async runtime**, not even as a
    /// dev-dependency: `discovery_hermetic`'s no-network guard (SC-013) asserts that the
    /// capability to speak a network protocol is *absent* here rather than merely unused,
    /// and an async runtime is the substrate a socket would need. Taking one on so a test
    /// could `.await` would trade that structural guarantee for convenience.
    ///
    /// It costs almost nothing to avoid. [`reduce`] is async only because the *live*
    /// predicate runs two CLI processes; the synthetic predicate below never suspends, so
    /// the whole future completes on its first poll and there is nothing to schedule. A
    /// `Poll::Pending` here would mean an await point that genuinely needs a scheduler,
    /// which is a fact worth a loud panic rather than a silent spin.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!(
                "the reduction future suspended. The hermetic tests drive it with a \
                 synthetic predicate that never awaits anything real, so a pending poll \
                 means a new await point that needs a scheduler — and this crate must not \
                 acquire one (SC-013)."
            ),
        }
    }

    fn reduce_now<P: ReproductionProbe>(
        input: &ReductionInput,
        budget: u64,
        probe: &mut P,
    ) -> Result<Reduction, P::Error> {
        block_on(reduce(input, budget, probe))
    }

    fn first_further_reduction_now<P: ReproductionProbe>(
        input: &ReductionInput,
        probe: &mut P,
    ) -> Result<Option<&'static str>, P::Error> {
        block_on(first_further_reduction(input, probe))
    }

    // -----------------------------------------------------------------------
    // A synthetic predicate (research D4/D5): no oracle, no process, no network
    // -----------------------------------------------------------------------

    fn signature(path: &str) -> Signature {
        Signature::derive(
            "chan-structured-output",
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: None,
                reference: None,
            },
        )
    }

    /// The signature under reduction in every test below.
    fn target() -> Signature {
        signature("configuration.remoteUser")
    }

    /// A predicate that reproduces exactly while a declared *witness condition* holds.
    ///
    /// Deliberately synthetic and deliberately declarative: the reduction strategy's job is
    /// to walk the catalogue and honor whatever the predicate says, and a predicate whose
    /// rule is written down in the test is the only way to assert that independently of an
    /// oracle's behavior on a particular day.
    struct Synthetic {
        /// Reproduces only while every one of these root keys is present.
        needs_keys: Vec<String>,
        /// …and only while this root key still holds a non-empty value.
        needs_non_empty: Option<String>,
        /// Documents that instead produce a DIFFERENT signature (FR-023).
        drifts_when_missing: Option<String>,
        /// Every document the predicate was asked about, in order — so a test can assert
        /// determinism over the probe SEQUENCE, not merely over the answer.
        seen: Vec<Value>,
    }

    impl Synthetic {
        fn requiring(keys: &[&str]) -> Synthetic {
            Synthetic {
                needs_keys: keys.iter().map(|k| (*k).to_string()).collect(),
                needs_non_empty: None,
                drifts_when_missing: None,
                seen: Vec::new(),
            }
        }

        fn and_non_empty(mut self, key: &str) -> Synthetic {
            self.needs_non_empty = Some(key.to_string());
            self
        }

        fn drifting_when_missing(mut self, key: &str) -> Synthetic {
            self.drifts_when_missing = Some(key.to_string());
            self
        }

        fn verdict(&self, document: &Value) -> Reproduction {
            let present = |key: &str| document.get(key).is_some();
            if let Some(key) = &self.drifts_when_missing
                && !present(key)
            {
                return Reproduction::Drifted(vec![signature("configuration.name")]);
            }
            if !self.needs_keys.iter().all(|k| present(k)) {
                return Reproduction::Absent;
            }
            if let Some(key) = &self.needs_non_empty {
                let non_empty = match document.get(key) {
                    Some(Value::Array(a)) => !a.is_empty(),
                    Some(Value::Object(o)) => !o.is_empty(),
                    Some(Value::String(s)) => !s.is_empty(),
                    Some(_) => true,
                    None => false,
                };
                if !non_empty {
                    return Reproduction::Absent;
                }
            }
            Reproduction::Preserved
        }
    }

    impl ReproductionProbe for Synthetic {
        type Error = std::convert::Infallible;

        async fn probe(&mut self, document: &Value) -> Result<Reproduction, Self::Error> {
            self.seen.push(document.clone());
            Ok(self.verdict(document))
        }
    }

    /// A document with something for every step to bite on.
    fn rich() -> Value {
        json!({
            "name": "Discovery Seed",
            "image": "alpine:3.19",
            "remoteUser": "vscode",
            "features": {
                "ghcr.io/devcontainers/features/git:1": { "version": "os-provided" },
                "ghcr.io/devcontainers/features/node:1": {}
            },
            "forwardPorts": [3000, 8080],
            "runArgs": ["--init", "--rm"],
            "containerEnv": { "A": "1", "B": "2" },
            "extends": ["./base.json", "./middle.json"],
            "runServices": ["app", "db", "cache"],
            "service": "app",
            "shutdownAction": "stopContainer"
        })
    }

    fn input(document: Value) -> ReductionInput {
        ReductionInput {
            document,
            mutations: Vec::new(),
            required_keys: vec!["image".to_string()],
        }
    }

    // -----------------------------------------------------------------------
    // T041 — signature preservation
    // -----------------------------------------------------------------------

    #[test]
    fn the_reduced_input_yields_the_same_signature_as_the_original() {
        let mut probe = Synthetic::requiring(&["image", "remoteUser"]);
        let start = input(rich());
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");

        // It still reproduces: the probe says so about the document that was returned.
        assert_eq!(
            probe.verdict(&reduction.document),
            Reproduction::Preserved,
            "the reduced input must reproduce the signature under reduction (FR-019)"
        );
        assert_eq!(
            target().id,
            target().derived_id(),
            "the target is well-formed"
        );

        // …and it really is reduced.
        assert!(
            reduction.reduced_size < reduction.original_size,
            "reduced {} nodes to {} — no reduction happened at all",
            reduction.original_size,
            reduction.reduced_size
        );
        assert!(
            reduction.size_reduction_fraction() >= 0.8,
            "reduced only {:.0}% of the input; SC-004 expects at least 80% on an input \
             this redundant. Reduced document: {}",
            reduction.size_reduction_fraction() * 100.0,
            reduction.document
        );
        assert!(
            !reduction.steps.is_empty(),
            "a reduction that reduced must name the steps it applied"
        );

        // Every step it names is a declared catalogue step, not a description someone
        // wrote at the call site.
        for step in &reduction.steps {
            assert!(
                REDUCTION_STEPS.contains(&step.as_str()),
                "`{step}` is not a declared catalogue step"
            );
        }
    }

    #[test]
    fn a_reduction_never_removes_a_key_the_predicate_still_needs() {
        let mut probe = Synthetic::requiring(&["image", "remoteUser"]).and_non_empty("runArgs");
        let start = input(rich());
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");

        assert!(reduction.document.get("image").is_some());
        assert!(reduction.document.get("remoteUser").is_some());
        assert!(
            matches!(reduction.document.get("runArgs"), Some(Value::Array(a)) if !a.is_empty()),
            "`runArgs` was emptied even though the predicate needs it non-empty: {}",
            reduction.document
        );
    }

    // -----------------------------------------------------------------------
    // T042 — minimality (FR-021)
    // -----------------------------------------------------------------------

    #[test]
    fn no_single_further_catalogue_step_reduces_a_minimal_result() {
        let mut probe = Synthetic::requiring(&["image", "remoteUser"]);
        let start = input(rich());
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");
        assert!(
            reduction.is_minimal,
            "a generous budget must reach the catalogue's fixed point: {:?}",
            reduction.not_minimal_reason
        );
        assert!(reduction.not_minimal_reason.is_none());

        // The independent check: a function whose only job is to look, rather than a
        // property inferred from how the loop happened to exit.
        let minimal = ReductionInput {
            document: reduction.document.clone(),
            mutations: Vec::new(),
            required_keys: start.required_keys.clone(),
        };
        let mut checker = Synthetic::requiring(&["image", "remoteUser"]);
        assert_eq!(
            first_further_reduction_now(&minimal, &mut checker).expect("infallible"),
            None,
            "step `{:?}` still reduces a result reported as minimal — FR-021's claim is \
             that NO single declared step does. Document: {}",
            first_further_reduction_now(
                &minimal,
                &mut Synthetic::requiring(&["image", "remoteUser"])
            )
            .expect("infallible"),
            reduction.document
        );

        // The check is not vacuous: it does find a reduction on an input that has one.
        let mut checker = Synthetic::requiring(&["image", "remoteUser"]);
        assert!(
            first_further_reduction_now(&start, &mut checker)
                .expect("infallible")
                .is_some(),
            "the minimality check found nothing to reduce on the UNREDUCED input, so its \
             verdict on the reduced one says nothing"
        );
    }

    // -----------------------------------------------------------------------
    // T043 — determinism (SC-004 / FR-020)
    // -----------------------------------------------------------------------

    #[test]
    fn the_same_finding_and_seed_yield_the_identical_minimal_input() {
        const SEED: u64 = 0x5EED_0043;

        // The seed enters through the mutation stream, which is what a real finding
        // carries: the reduction itself draws from no PRNG at all, so it is
        // seed-INDEPENDENT — a stronger property than FR-020 asks for, and the reason the
        // same finding cannot reduce two ways.
        let mutated = |seed: u64| -> ReductionInput {
            let mut prng = Prng::from_seed(seed);
            let applied = mutate::apply(MutationCategory::UnknownField, &rich(), &mut prng)
                .expect("the operator has a target");
            ReductionInput {
                document: applied.document,
                mutations: vec![applied.mutation],
                required_keys: vec!["image".to_string()],
            }
        };

        let first = reduce_now(
            &mutated(SEED),
            512,
            &mut Synthetic::requiring(&["image", "remoteUser"]),
        )
        .expect("infallible");
        let second = reduce_now(
            &mutated(SEED),
            512,
            &mut Synthetic::requiring(&["image", "remoteUser"]),
        )
        .expect("infallible");

        assert_eq!(
            first.document, second.document,
            "the reduced input differed"
        );
        assert_eq!(
            first.steps, second.steps,
            "the applied step SEQUENCE differed"
        );
        assert_eq!(first.probes, second.probes, "the probe count differed");
        assert_eq!(first.is_minimal, second.is_minimal);
        assert_eq!(first.remaining_mutations, second.remaining_mutations);

        // The probe SEQUENCE is identical too, not merely its final answer: a shrinker
        // that reached the same fixed point by a different route would still be
        // non-deterministic in the thing FR-020 bounds — the work a reviewer's
        // reproduction has to repeat.
        let mut left = Synthetic::requiring(&["image", "remoteUser"]);
        let mut right = Synthetic::requiring(&["image", "remoteUser"]);
        reduce_now(&mutated(SEED), 512, &mut left).expect("ok");
        reduce_now(&mutated(SEED), 512, &mut right).expect("ok");
        assert_eq!(left.seen, right.seen);
        assert!(
            !left.seen.is_empty(),
            "no probe was ever made, so equality of the sequences is vacuous"
        );
    }

    // -----------------------------------------------------------------------
    // T044 — budget exhaustion (FR-022)
    // -----------------------------------------------------------------------

    #[test]
    fn an_exhausted_budget_emits_the_best_reduction_marked_not_minimal_with_a_reason() {
        let mut probe = Synthetic::requiring(&["image", "remoteUser"]);
        let start = input(rich());
        let reduction = reduce_now(&start, 2, &mut probe).expect("infallible");

        assert!(
            !reduction.is_minimal,
            "a two-probe budget cannot exhaust the catalogue on this input, so claiming \
             minimality would be presenting a partially reduced input as minimal — exactly \
             what FR-022 forbids"
        );
        let reason = reduction
            .not_minimal_reason
            .as_deref()
            .expect("not-minimal is never reported without a reason (FR-022)");
        assert!(
            reason.contains("budget") && reason.contains('2'),
            "the reason must name the budget that ran out: {reason}"
        );
        assert!(
            reduction.probes <= 2,
            "the budget bounds PROBES, and {} were spent against a budget of 2",
            reduction.probes
        );

        // "Best reduction found" — not the untouched input, and not nothing.
        assert!(
            reduction.reduced_size < reduction.original_size,
            "an exhausted budget must still emit the best reduction it reached"
        );
        assert_eq!(
            probe.verdict(&reduction.document),
            Reproduction::Preserved,
            "even a partial reduction must still reproduce the signature"
        );

        // And the same input with a generous budget IS minimal, so the flag tracks the
        // budget rather than being permanently false.
        let mut generous = Synthetic::requiring(&["image", "remoteUser"]);
        let full = reduce_now(&start, 512, &mut generous).expect("infallible");
        assert!(full.is_minimal);
        assert!(full.reduced_size <= reduction.reduced_size);
    }

    // -----------------------------------------------------------------------
    // T045 — signature drift (FR-023)
    // -----------------------------------------------------------------------

    #[test]
    fn a_step_that_changes_the_signature_is_rejected_and_the_new_one_is_captured() {
        // Dropping `name` produces a DIFFERENT signature rather than removing the
        // difference: the step must be rejected for the finding under reduction, and the
        // signature it produced instead must survive as a candidate finding.
        let mut probe =
            Synthetic::requiring(&["image", "remoteUser"]).drifting_when_missing("name");
        let start = input(rich());
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");

        assert!(
            reduction.document.get("name").is_some(),
            "the drifting step was ACCEPTED: the reduced input no longer reproduces the \
             finding it claims to be about. Document: {}",
            reduction.document
        );
        assert_eq!(
            reduction.drifted.len(),
            1,
            "the new signature must be captured as a separate candidate finding (FR-023), \
             got {:?}",
            reduction.drifted
        );
        let captured = &reduction.drifted[0];
        assert_eq!(captured.signature.path, "configuration.name");
        assert_eq!(
            captured.signature.id,
            signature("configuration.name").id,
            "the captured signature must be the one the probe reported, verbatim"
        );
        assert_ne!(
            captured.signature.id,
            target().id,
            "a drifted signature that equalled the target would not be drift at all"
        );

        // It carries the input it was SEEN on, not the document this reduction settled
        // on. The rejected proposal is what reproduces the drift; the reduction went
        // elsewhere precisely because that proposal did not preserve the target.
        assert_eq!(
            probe.verdict(&captured.document),
            Reproduction::Drifted(vec![signature("configuration.name")]),
            "the captured input must reproduce the drifted signature, or the new finding \
             names an input nobody can re-examine"
        );
        assert_ne!(
            captured.document, reduction.document,
            "the drifted input is the REJECTED proposal, which is not where the reduction \
             ended up"
        );

        // Deduplicated: the same drift observed by several rejected proposals is one
        // candidate finding, not one per probe.
        assert_eq!(
            reduction
                .drifted
                .iter()
                .map(|d| d.signature.id.clone())
                .collect::<std::collections::BTreeSet<String>>()
                .len(),
            reduction.drifted.len()
        );

        // The reduction still did its job around the rejected step.
        assert_eq!(probe.verdict(&reduction.document), Reproduction::Preserved);
        assert!(reduction.reduced_size < reduction.original_size);
    }

    // -----------------------------------------------------------------------
    // Step behaviors, individually
    // -----------------------------------------------------------------------

    #[test]
    fn un_apply_mutation_reverses_exactly_one_recorded_operator() {
        let mut prng = Prng::from_seed(0x5EED_0047);
        let applied = mutate::apply(MutationCategory::UnknownField, &rich(), &mut prng)
            .expect("the operator has a target");
        let start = ReductionInput {
            document: applied.document.clone(),
            mutations: vec![applied.mutation.clone()],
            required_keys: vec!["image".to_string()],
        };

        let mut probe = Synthetic::requiring(&["image", "remoteUser"]);
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");

        assert!(
            reduction.remaining_mutations.is_empty(),
            "the mutation was not needed to reproduce, so it must have been un-applied: {:?}",
            reduction.remaining_mutations
        );
        assert!(
            reduction.steps.iter().any(|s| s == "un-apply-mutation"),
            "the reversal must be attributed to its catalogue step: {:?}",
            reduction.steps
        );
        for (key, _) in &applied.mutation.reversal.keys {
            assert!(
                reduction.document.get(key).is_none()
                    || rich().get(key) == reduction.document.get(key),
                "`{key}` still carries the mutation's value after un-applying it"
            );
        }
    }

    #[test]
    fn collapse_extends_level_shortens_the_chain_and_then_removes_the_link() {
        let document = json!({ "image": "alpine:3.19", "extends": ["./a.json", "./b.json"] });
        let shortened = collapse_extends(&document);
        assert_eq!(
            shortened,
            vec![
                json!({ "image": "alpine:3.19", "extends": ["./a.json"] }),
                json!({ "image": "alpine:3.19" }),
            ],
            "a chain of two loses one level first, and the link second"
        );

        let single = json!({ "image": "alpine:3.19", "extends": "./a.json" });
        assert_eq!(
            collapse_extends(&single),
            vec![json!({ "image": "alpine:3.19" })],
            "a chain of one has no level to inline, so the collapse IS removing the link"
        );

        assert!(collapse_extends(&json!({ "image": "alpine:3.19" })).is_empty());
    }

    #[test]
    fn drop_compose_service_never_drops_the_service_the_document_names() {
        let document = json!({
            "dockerComposeFile": "docker-compose.yml",
            "service": "app",
            "runServices": ["app", "db", "cache"]
        });
        let proposed = drop_compose_service(&document);
        assert_eq!(
            proposed.len(),
            2,
            "two droppable services, got {proposed:?}"
        );
        for candidate in &proposed {
            let services = candidate["runServices"]
                .as_array()
                .expect("still an array")
                .clone();
            assert!(
                services.contains(&json!("app")),
                "the referenced service was dropped: {candidate}"
            );
            assert_eq!(services.len(), 2);
        }

        // Nothing to do without a `runServices` list.
        assert!(drop_compose_service(&json!({ "service": "app" })).is_empty());
    }

    #[test]
    fn minimize_scalar_preserves_the_json_type() {
        for (before, after) in [
            (json!("something"), json!("")),
            (json!(4242), json!(0)),
            (json!(true), json!(false)),
        ] {
            assert_eq!(minimal_scalar(&before), Some(after));
        }
        assert_eq!(
            minimal_scalar(&json!(null)),
            None,
            "null is already minimal"
        );
        assert_eq!(
            minimal_scalar(&json!([1])),
            None,
            "an array is not a scalar"
        );
        assert_eq!(
            minimal_scalar(&json!({})),
            None,
            "an object is not a scalar"
        );
    }

    #[test]
    fn drop_feature_removes_one_entry_at_a_time() {
        let document = json!({
            "image": "alpine:3.19",
            "features": { "a": {}, "b": {}, "c": {} }
        });
        let ids = feature_ids(&document);
        assert_eq!(
            ids,
            vec!["a", "b", "c"],
            "features are visited in map order"
        );
        for id in &ids {
            let reduced = remove_feature(&document, id).expect("removable");
            let features = reduced["features"].as_object().expect("object");
            assert_eq!(features.len(), 2);
            assert!(!features.contains_key(id));
        }
        assert!(remove_feature(&document, "not-there").is_none());
    }

    #[test]
    fn a_required_key_is_never_dropped() {
        let start = ReductionInput {
            document: json!({ "image": "alpine:3.19", "name": "x", "remoteUser": "vscode" }),
            mutations: Vec::new(),
            // Everything is "required" here, so a reduction that respects the grammar has
            // nothing to drop and the loop must terminate immediately as minimal.
            required_keys: vec![
                "image".to_string(),
                "name".to_string(),
                "remoteUser".to_string(),
            ],
        };
        let mut probe = Synthetic::requiring(&[]);
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");
        for key in ["image", "name", "remoteUser"] {
            assert!(
                reduction.document.get(key).is_some(),
                "the grammar marks `{key}` required, and a reduction that violates the \
                 grammar produces an input the finding was never found on"
            );
        }
        assert!(reduction.is_minimal);
    }

    // -----------------------------------------------------------------------
    // Termination, the property the whole loop rests on
    // -----------------------------------------------------------------------

    #[test]
    fn every_accepted_step_strictly_decreases_the_objective() {
        // The guard against an emptying/un-applying oscillation. `un-apply-mutation` can
        // GROW the document, so a size-only objective would let the loop cycle forever,
        // spending the expensive step on a ring it never leaves.
        let mut prng = Prng::from_seed(0x5EED_0048);
        let applied = mutate::apply(MutationCategory::EmptyValue, &rich(), &mut prng)
            .expect("the operator has a target");
        let before = complexity(&applied.document, 1);
        let reversed = applied.mutation.reversal.apply(&applied.document);
        let after = complexity(&reversed, 0);

        assert!(
            after < before,
            "un-applying must count as progress even when it grows the document: \
             {before:?} → {after:?}"
        );
        assert!(
            node_count(&reversed) >= node_count(&applied.document),
            "this assertion is vacuous unless the reversal really did grow the document"
        );
    }

    #[test]
    fn a_probe_failure_aborts_the_reduction_rather_than_reading_as_a_rejection() {
        struct Failing;
        impl ReproductionProbe for Failing {
            type Error = &'static str;
            async fn probe(&mut self, _document: &Value) -> Result<Reproduction, Self::Error> {
                Err("the comparison could not be run")
            }
        }
        let start = input(rich());
        let error =
            reduce_now(&start, 512, &mut Failing).expect_err("a probe failure must propagate");
        assert_eq!(error, "the comparison could not be run");
    }

    #[test]
    fn a_skipped_reduction_is_never_presented_as_minimal() {
        let start = input(rich());
        let skipped = Reduction::not_attempted(&start, "already carried by the standing queue");
        assert!(
            !skipped.is_minimal,
            "an UNREDUCED input presented as minimal is the same lie FR-022 forbids about a \
             partially reduced one"
        );
        assert_eq!(
            skipped.not_minimal_reason.as_deref(),
            Some("already carried by the standing queue")
        );
        assert_eq!(skipped.document, start.document);
        assert_eq!(skipped.probes, 0);
        assert!(skipped.steps.is_empty());
        assert_eq!(skipped.original_size, skipped.reduced_size);
        assert_eq!(skipped.size_reduction_fraction(), 0.0);
    }

    #[test]
    fn an_empty_document_reduces_to_itself_and_reports_no_division_by_zero() {
        let start = ReductionInput {
            document: json!({}),
            mutations: Vec::new(),
            required_keys: Vec::new(),
        };
        let mut probe = Synthetic::requiring(&[]);
        let reduction = reduce_now(&start, 512, &mut probe).expect("infallible");
        assert_eq!(reduction.document, json!({}));
        assert!(reduction.is_minimal);
        assert_eq!(reduction.probes, 0);
        assert!(reduction.size_reduction_fraction().is_finite());
        assert_eq!(reduction.size_reduction_fraction(), 0.0);
    }
}
