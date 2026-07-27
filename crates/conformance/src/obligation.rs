//! Obligation (`obl-`) identity, generation, and disposition (`odp-`) resolution
//! (024-deterministic-conformance-coverage, contracts/obligation.md).
//!
//! This module owns obligation **identity** (T012/T013) and **generation**
//! (T037/T038). Disposition resolution (T061–T063) lands in User Story 2 and builds on
//! the records here.
//!
//! ## Identity is substance-anchored
//!
//! Following the `clu-` clause precedent, an obligation's id is derived from what it
//! *is*, never from where it sits:
//!
//! | Kind | Id | Hashed over |
//! |---|---|---|
//! | behavior | `obl-bhv-<hash8>` | `behavior ‖ canonical(context)` |
//! | combination | `obl-cmb-<hash8>` | `operation ‖ canonical(sorted assignment)` |
//!
//! The point of canonicalizing is that a **cosmetic** edit must not orphan a
//! hand-authored disposition. Reordering records, renaming a file, moving a dimension's
//! declaration position, or writing the same pair's keys in a different order all leave
//! the id alone. Changing what a combination *is* does change the id — and that is a new
//! obligation needing its own decision, which is correct.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::load::Registry;
use crate::model::{Condition, RevisionKind};
use crate::scenario::{HighRiskTriple, OPERATION_DIMENSION, ScenarioModel, is_invalid};

/// Field separator for hash inputs: ASCII Unit Separator, the same byte
/// [`crate::inventory`] uses. Registry ids and dimension values are printable ASCII,
/// so no input can contain it and the concatenation stays injective — `("ab", "c")`
/// can never hash as `("a", "bc")`.
const HASH_SEPARATOR: char = '\u{1f}';

/// Separator BETWEEN the values inside one condition's value set: ASCII Record
/// Separator, distinct from [`HASH_SEPARATOR`].
///
/// A distinct byte is what makes [`canonical_context`] injective. With one separator
/// everywhere, `dim ‖ v₁ ‖ … ‖ vₙ` has no arity marker, so
/// `[{a: [b]}, {c: [d]}]` and `[{a: [b, c, d]}]` both render `a␟b␟c␟d␟` — two
/// semantically distinct behavior obligations colliding on one `obl-bhv-<hash8>` id,
/// silently transferring one behavior's hand-authored `odp-` disposition to the other.
/// Using `␞` within a value set makes the `␟` that closes it unambiguous.
const VALUE_SEPARATOR: char = '\u{1e}';

/// The first 8 lowercase-hex chars of SHA-256 over `parts`, joined by
/// [`HASH_SEPARATOR`].
///
/// Deliberately self-contained rather than reaching for `inventory::hash8` or
/// `clause::hash8`: those are private and differently shaped (they hash a schema
/// pointer and a prose excerpt respectively), and coupling to either would tie an
/// obligation's identity to an unrelated record's field set. Only the *truncation
/// convention* is shared, so ids read alike across the registry.
fn hash8(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (idx, part) in parts.iter().enumerate() {
        if idx > 0 {
            hasher.update([HASH_SEPARATOR as u8]);
        }
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(8);
    for b in &digest[..4] {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Canonicalize an environment context for hashing: conditions sorted by dimension
/// (then by their values, so the order is total even for a malformed context naming one
/// dimension twice), and each condition's value SUBSET sorted.
///
/// Both sorts are semantic, not cosmetic tolerance for its own sake: a context is a
/// conjunction of conditions and each condition pins a value *set*, so neither order
/// carries meaning and neither may perturb the id.
///
/// The encoding is injective: values are joined by [`VALUE_SEPARATOR`] and each
/// condition is closed by [`HASH_SEPARATOR`], so a value set's extent is unambiguous
/// regardless of arity (see [`VALUE_SEPARATOR`] for the collision this avoids).
fn canonical_context(context: &[Condition]) -> String {
    let mut conditions: Vec<(String, Vec<String>)> = context
        .iter()
        .map(|c| {
            let mut values = c.values.clone();
            values.sort();
            (c.dimension.clone(), values)
        })
        .collect();
    conditions.sort();

    let mut out = String::new();
    for (dimension, values) in conditions {
        out.push_str(&dimension);
        out.push(HASH_SEPARATOR);
        out.push_str(&values.join(&VALUE_SEPARATOR.to_string()));
        out.push(HASH_SEPARATOR);
    }
    out
}

/// Canonicalize a scenario-dimension assignment for hashing: pairs sorted by key.
///
/// **The function sorts; the caller never has to.** contracts/obligation.md is explicit
/// that two authors writing the same pair in different key order must produce the same
/// id, because otherwise a disposition would silently detach the first time someone
/// reformatted a file. Accepting any iterable of pairs — an `IndexMap` in declaration
/// order, a `BTreeMap`, a plain `Vec` — means no caller can accidentally opt out of that
/// guarantee by picking an order-preserving container.
fn canonical_assignment<K: AsRef<str>, V: AsRef<str>, I: IntoIterator<Item = (K, V)>>(
    assignment: I,
) -> String {
    let mut pairs: Vec<(String, String)> = assignment
        .into_iter()
        .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
        .collect();
    pairs.sort();

    let mut out = String::new();
    for (key, value) in pairs {
        out.push_str(&key);
        out.push(HASH_SEPARATOR);
        out.push_str(&value);
        out.push(HASH_SEPARATOR);
    }
    out
}

/// The `obl-bhv-<hash8>` id for a behavior paired with a context its own applicability
/// requires (contracts/obligation.md, data-model.md §4).
///
/// Hashed over `behavior ‖ canonical(context)`. Reordering the conditions, or the values
/// within a condition, does not change the id; naming a different behavior or pinning a
/// different value set does.
pub fn behavior_obligation_id(behavior: &str, context: &[Condition]) -> String {
    format!(
        "obl-bhv-{}",
        hash8(&[behavior, &canonical_context(context)])
    )
}

/// The `obl-cmb-<hash8>` id for a valid scenario-dimension combination — a pair, or a
/// selected high-risk triple — under one operation (contracts/obligation.md,
/// data-model.md §4).
///
/// Hashed over `operation ‖ canonical(sorted assignment)`. The assignment keys are
/// sorted **inside** this function, so authoring order can never fork an id.
pub fn combination_obligation_id<K, V, I>(operation: &str, assignment: I) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
    I: IntoIterator<Item = (K, V)>,
{
    format!(
        "obl-cmb-{}",
        hash8(&[operation, &canonical_assignment(assignment)])
    )
}

/// The `obl-cmb-` id [`generate_triples`] derives from a high-risk triple, or `None` when
/// the triple names no operation (a degenerate record V26 reports; generation skips it
/// rather than invent a partition key).
///
/// Exposed so the triples report joins on the SAME id generation emits, rather than
/// re-deriving the identity scheme in a second place where the two could drift.
pub fn triple_obligation_id(triple: &HighRiskTriple) -> Option<String> {
    let operation = triple.assignment.get(OPERATION_DIMENSION)?;
    Some(combination_obligation_id(
        operation,
        triple
            .assignment
            .iter()
            .filter(|(k, _)| k.as_str() != OPERATION_DIMENSION),
    ))
}

// ---------------------------------------------------------------------------
// Records — `conformance/obligations/obligations.json` (machine-owned)
// ---------------------------------------------------------------------------

/// Schema version of the committed obligation inventory. Bumped only on a breaking
/// shape change.
pub const OBLIGATION_SCHEMA_VERSION: u32 = 1;

/// The two obligation kinds, never multiplied together (FR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationKind {
    /// A behavior paired with a context its own applicability requires.
    Behavior,
    /// A valid pair, or a selected high-risk triple, of scenario-dimension values.
    Combination,
}

/// One generated obligation (`obl-`) — data-model.md §4.
///
/// The two kinds share one record type (and therefore one disposition vocabulary) but
/// populate disjoint field groups: a `combination` carries `operation`/`assignment`/
/// `arity`, a `behavior` carries `behavior`/`context`. Field order here IS the wire
/// order, so the rendered file reads the way data-model.md §4 writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Obligation {
    /// `obl-bhv-<hash8>` or `obl-cmb-<hash8>`; substance-anchored.
    pub id: String,
    pub kind: ObligationKind,
    /// Combination only: the partitioning operation (FR-013a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Combination only: the pinned scenario-dimension values, excluding the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment: Option<IndexMap<String, String>>,
    /// Combination only: `2` for a pair, `3` for a high-risk triple.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arity: Option<u32>,
    /// Behavior only: the `bhv-` this obligation is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
    /// Behavior only: the **environment** context its applicability requires; `[]` means
    /// "applies everywhere" and is serialized (an absent context and an empty one are
    /// different claims).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<Condition>>,
}

impl Obligation {
    /// Whether this obligation is a high-risk triple (`arity: 3`) — the one kind
    /// FR-015 forbids satisfying by rationale or waiver.
    pub fn is_triple(&self) -> bool {
        self.arity == Some(3)
    }
}

/// The committed obligation inventory — `conformance/obligations/obligations.json`.
///
/// Machine-owned: the sole output of `coverage generate`. A hand edit is
/// indistinguishable from staleness and is caught by the same V27 byte-comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObligationInventory {
    pub schema_version: u32,
    /// The registry's `spec`-kind revision id — the provenance pin (V27).
    pub revision: String,
    /// Every obligation, **sorted by id** so declaration order of the inputs is
    /// irrelevant to the output.
    pub units: Vec<Obligation>,
}

/// Why obligation generation could not produce an inventory. Generation is otherwise
/// total: a malformed *model* is reported by V26, not here, so that `validate` names the
/// modelling mistake rather than a downstream symptom.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObligationError {
    /// No `spec`-kind revision to pin the inventory against — the inventory would have
    /// no provenance, so it is refused rather than written with a guessed pin.
    #[error(
        "the registry declares no `spec`-kind revision, so the obligation inventory has \
         no revision to pin against"
    )]
    NoSpecRevision,
}

// ---------------------------------------------------------------------------
// Generation (contracts/obligation.md "Generation is total and ordered")
// ---------------------------------------------------------------------------

/// Generate the complete obligation set for `registry` (T037/T038).
///
/// ```text
/// 1. for each operation o          (declaration order of sdim-operation)
/// 2.   prune dimensions inapplicable under o
/// 3.   for each unordered pair of remaining dimensions
/// 4.     for each value pair not excluded by a rule
/// 5.       emit obl-cmb (arity 2)
/// 6. for each high-risk triple      (declaration order)
/// 7.   emit obl-cmb (arity 3)
/// 8. for each behavior              (id order)
/// 9.   for each context its applicability requires
/// 10.     emit obl-bhv
/// 11. sort all units by id
/// ```
///
/// Step 11 makes declaration order irrelevant to the output, so the file is stable under
/// reformatting of its inputs. Nothing here reads the clock, the filesystem layout, or a
/// hash map's iteration order.
///
/// **The full Cartesian product is never materialized.** Pairs are enumerated directly
/// from the two-dimension cross product of *permitted* values, which is what makes the
/// space tractable without a covering-array minimizer (research Decision 3).
///
/// **Environment dimensions never enter step 1** (FR-013b): they determine whether an
/// obligation is *runnable*, not what it exercises.
pub fn generate_obligations(registry: &Registry) -> Result<ObligationInventory, ObligationError> {
    let revision = registry
        .revisions
        .iter()
        .find(|r| r.kind == RevisionKind::Spec)
        .map(|r| r.id.clone())
        .ok_or(ObligationError::NoSpecRevision)?;

    let model = ScenarioModel::new(&registry.scenario, &registry.applicability);
    let mut units: Vec<Obligation> = Vec::new();

    units.extend(generate_pairs(&model));
    units.extend(generate_triples(&registry.triples));
    units.extend(generate_behavior_obligations(registry));

    // Step 11. Stable sort so a (V2-invalid) duplicate id keeps its emission order and
    // both records stay visible instead of one silently winning.
    units.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(ObligationInventory {
        schema_version: OBLIGATION_SCHEMA_VERSION,
        revision,
        units,
    })
}

/// Steps 1–5: the per-operation pairwise enumeration.
fn generate_pairs(model: &ScenarioModel<'_>) -> Vec<Obligation> {
    let mut out = Vec::new();
    let Some(operations) = model.operation_dimension() else {
        // A model with no operation dimension has no partition key. Emitting pairs
        // against a guessed key would fabricate a denominator; V26 reports the model.
        return out;
    };

    for operation in &operations.values {
        let applicable = model.applicable_dimensions(operation);
        for (i, (first, first_values)) in applicable.iter().enumerate() {
            for (second, second_values) in applicable.iter().skip(i + 1) {
                for a in first_values {
                    for b in second_values {
                        let combination = [
                            (OPERATION_DIMENSION, operation.as_str()),
                            (first.id.as_str(), *a),
                            (second.id.as_str(), *b),
                        ];
                        // The per-value pruning above already removed values excluded
                        // with the operation alone; this catches a rule naming BOTH
                        // pair members (or all three dimensions).
                        if is_invalid(model.rules, &combination) {
                            continue;
                        }
                        let mut assignment = IndexMap::new();
                        assignment.insert(first.id.clone(), (*a).to_string());
                        assignment.insert(second.id.clone(), (*b).to_string());
                        out.push(Obligation {
                            id: combination_obligation_id(operation, &assignment),
                            kind: ObligationKind::Combination,
                            operation: Some(operation.clone()),
                            assignment: Some(assignment),
                            arity: Some(2),
                            behavior: None,
                            context: None,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Steps 6–7: one `arity: 3` combination obligation per hand-selected high-risk triple.
///
/// The triple's own `assignment` carries the operation alongside the three dimensions it
/// pins (data-model.md §5); the obligation splits them so a triple and a pair under the
/// same operation share one identity scheme. A triple that names no operation is skipped
/// here and reported by V20-style arity validation rather than being emitted against an
/// invented partition key.
fn generate_triples(triples: &[HighRiskTriple]) -> Vec<Obligation> {
    let mut out = Vec::new();
    for triple in triples {
        let (Some(operation), Some(id)) = (
            triple.assignment.get(OPERATION_DIMENSION),
            triple_obligation_id(triple),
        ) else {
            continue;
        };
        let assignment: IndexMap<String, String> = triple
            .assignment
            .iter()
            .filter(|(k, _)| k.as_str() != OPERATION_DIMENSION)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.push(Obligation {
            id,
            kind: ObligationKind::Combination,
            operation: Some(operation.clone()),
            assignment: Some(assignment),
            arity: Some(3),
            behavior: None,
            context: None,
        });
    }
    out
}

/// Steps 8–10: one behavior obligation per behavior, carrying the context its own
/// `applicability` requires.
///
/// **Exactly one obligation per behavior**, deliberately:
///
/// - An **empty** `applicability` means "applies everywhere" — one universal context, so
///   one obligation with an empty context. Zero obligations would erase the behavior from
///   the denominator; one per dimension value would invent contexts the behavior never
///   distinguishes.
/// - A **non-empty** `applicability` is itself the context: each condition pins a value
///   *subset*, meaning "any of these", not "each of these separately". Expanding a subset
///   into one obligation per value would multiply the two obligation kinds against the
///   environment model — precisely the multiplication FR-019 forbids, and the arithmetic
///   research Decision 2 rejected.
fn generate_behavior_obligations(registry: &Registry) -> Vec<Obligation> {
    let mut behaviors: Vec<&crate::model::BehaviorUnit> = registry.behaviors.iter().collect();
    behaviors.sort_by(|a, b| a.id.cmp(&b.id));
    behaviors
        .into_iter()
        .map(|behavior| Obligation {
            id: behavior_obligation_id(&behavior.id, &behavior.applicability),
            kind: ObligationKind::Behavior,
            operation: None,
            assignment: None,
            arity: None,
            behavior: Some(behavior.id.clone()),
            context: Some(behavior.applicability.clone()),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Disposition (`odp-`) — `registry/obligation-dispositions/<area>.json` (T061–T063)
// ---------------------------------------------------------------------------

/// The four dispositions an obligation may carry (data-model.md §6). Closed: there is no
/// "different but acceptable" state, and no fifth word may be invented to soften a gap.
///
/// `inactive-environment` is deliberately **absent**: it is a *reporting bucket* derived
/// from the active profile, never a hand-authored judgement (contracts/obligation.md,
/// "Rules that make the vocabulary honest", rule 4). Putting it here would let an author
/// retire an obligation by declaring its environment inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DispositionKind {
    /// Satisfied by ≥1 resolvable, executable case. Never blocks.
    Case,
    /// Declared untestable, with a `rationale` naming a ground. Never blocks.
    NonTestable,
    /// Covered by a resolvable `wvr-` waiver. Blocks only once that waiver expires (V6).
    Waived,
    /// Admitted missing coverage, pointing at a resolvable `gap-`. **Always** blocks,
    /// because the gap record it names always blocks.
    Gap,
}

/// One hand-authored obligation disposition (`odp-`) — data-model.md §6.
///
/// Exactly one of `cases` / `rationale` / `waiver` / `gap` is present, and which one is
/// fixed by `disposition`. **That arity is enforced at deserialize time** via
/// [`RawObligationDisposition`], not deferred to V28/V29, for the reason `residual.rs`
/// gives for the same choice: a record whose disposition and payload disagree is not a
/// nuance a validation pass should interpret — it is a half-stated judgement, and the
/// only honest reading is to refuse it at the door. V28 then owns *arity across records*
/// (how many dispositions one obligation has) and V29 owns *semantics* (filler rationale,
/// a triple argued rather than tested, a stale or dangling reference) — questions that
/// genuinely need the rest of the registry to answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "RawObligationDisposition")]
pub struct ObligationDisposition {
    /// `odp-<slug>`.
    pub id: String,
    /// The `obl-` id this record dispositions. A disposition whose obligation no longer
    /// resolves is **stale** (V29) — reported, never quietly dropped.
    pub obligation: String,
    pub disposition: DispositionKind,
    /// `case` only: the covering case ids, ≥1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<String>,
    /// `non-testable` only: the ground. A filler phrase is V29.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// `waived` only: the backing `wvr-` id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiver: Option<String>,
    /// `gap` only: the backing `gap-` id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<String>,
}

/// The on-disk shape, validated into [`ObligationDisposition`] by [`TryFrom`]. Carries
/// `deny_unknown_fields` because it is the struct that actually reads the file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawObligationDisposition {
    id: String,
    obligation: String,
    disposition: DispositionKind,
    #[serde(default)]
    cases: Vec<String>,
    #[serde(default)]
    rationale: Option<String>,
    #[serde(default)]
    waiver: Option<String>,
    #[serde(default)]
    gap: Option<String>,
}

impl TryFrom<RawObligationDisposition> for ObligationDisposition {
    type Error = String;

    fn try_from(raw: RawObligationDisposition) -> Result<Self, Self::Error> {
        let id = raw.id;
        let rationale = disposition_value(raw.rationale.as_deref(), "rationale", &id)?;
        let waiver = disposition_value(raw.waiver.as_deref(), "waiver", &id)?;
        let gap = disposition_value(raw.gap.as_deref(), "gap", &id)?;
        for (index, case) in raw.cases.iter().enumerate() {
            disposition_value(Some(case), &format!("cases[{index}]"), &id)?;
        }

        // Which payload each disposition REQUIRES, and — by exclusion — which three it
        // forbids. Stated once, as data, so the two halves cannot drift apart.
        let present: Vec<&str> = [
            (!raw.cases.is_empty()).then_some("cases"),
            rationale.is_some().then_some("rationale"),
            waiver.is_some().then_some("waiver"),
            gap.is_some().then_some("gap"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let required = match raw.disposition {
            DispositionKind::Case => "cases",
            DispositionKind::NonTestable => "rationale",
            DispositionKind::Waived => "waiver",
            DispositionKind::Gap => "gap",
        };
        if present != [required] {
            return Err(format!(
                "obligation disposition `{id}` is `{}` so it must carry exactly \
                 `{required}` and nothing else, but it carries [{}]. A disposition whose \
                 payload disagrees with its word is a half-stated judgement: say which of \
                 the four it is, and back only that one.",
                disposition_name(raw.disposition),
                if present.is_empty() {
                    "nothing".to_string()
                } else {
                    present.join(", ")
                }
            ));
        }

        Ok(ObligationDisposition {
            id,
            obligation: raw.obligation,
            disposition: raw.disposition,
            cases: raw.cases,
            rationale,
            waiver,
            gap,
        })
    }
}

/// The wire spelling of a disposition, for diagnostics.
pub fn disposition_name(kind: DispositionKind) -> &'static str {
    match kind {
        DispositionKind::Case => "case",
        DispositionKind::NonTestable => "non-testable",
        DispositionKind::Waived => "waived",
        DispositionKind::Gap => "gap",
    }
}

/// Normalize an optional payload field: absent stays absent, but a blank value or the
/// scaffold sentinel is refused (T070).
///
/// A blank field is an authoring mistake, not an intentional absence — treating the two
/// alike would let `"rationale": ""` satisfy `non-testable`. The sentinel check is the
/// half that makes `coverage scaffold` safe to run: its output is deliberately
/// unloadable, so a skeleton committed unedited fails at the door rather than certifying
/// as a considered judgement.
fn disposition_value(value: Option<&str>, field: &str, id: &str) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(format!(
                    "obligation disposition `{id}`: `{field}` is present but blank — omit \
                     the field entirely to mean \"absent\", so a blank value can never \
                     stand in for a real one"
                ));
            }
            if trimmed == crate::residual::UNREVIEWED_SENTINEL {
                return Err(format!(
                    "obligation disposition `{id}`: `{field}` carries the `{}` scaffold \
                     sentinel — a scaffolded record must be reviewed and filled in before \
                     it is committed",
                    crate::residual::UNREVIEWED_SENTINEL
                ));
            }
            Ok(Some(raw.to_string()))
        }
    }
}

/// How many dispositions an obligation carries, and which (T063).
///
/// **Explicit only. No inheritance, no default.** There is no document-scope fallback of
/// the kind `clc-` classifications use for authoring documents, and no implicit
/// "evidence exists, so call it covered". An obligation with no record is
/// [`Resolution::Undispositioned`] — a state nobody chose, which V28 refuses — and one
/// with several is [`Resolution::Conflicting`], which V28 also refuses. Resolution never
/// picks a winner from a conflict: silently preferring one record would convert a
/// disagreement between two reviewers into a decision neither made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution<'a> {
    /// No `odp-` record names this obligation.
    Undispositioned,
    /// Exactly one record — the only resolvable state.
    One(&'a ObligationDisposition),
    /// More than one record, id-sorted.
    Conflicting(Vec<&'a ObligationDisposition>),
}

impl<'a> Resolution<'a> {
    /// The single disposition, or `None` for both the zero and the many case.
    pub fn single(&self) -> Option<&'a ObligationDisposition> {
        match self {
            Resolution::One(record) => Some(record),
            _ => None,
        }
    }
}

/// An obligation-id → disposition-record index over a registry's hand-authored records.
///
/// Built once and shared by the reporter and both validators, so "what dispositions this
/// obligation" has exactly one answer in the codebase.
#[derive(Debug, Clone, Default)]
pub struct DispositionIndex<'a> {
    by_obligation: std::collections::BTreeMap<&'a str, Vec<&'a ObligationDisposition>>,
}

impl<'a> DispositionIndex<'a> {
    /// Index every `odp-` record in `registry`, id-sorting the records under each
    /// obligation so a conflict is reported in a stable order.
    pub fn build(registry: &'a Registry) -> DispositionIndex<'a> {
        let mut by_obligation: std::collections::BTreeMap<&str, Vec<&ObligationDisposition>> =
            std::collections::BTreeMap::new();
        for record in &registry.obligation_dispositions {
            by_obligation
                .entry(record.obligation.as_str())
                .or_default()
                .push(record);
        }
        for records in by_obligation.values_mut() {
            records.sort_by(|a, b| a.id.cmp(&b.id));
        }
        DispositionIndex { by_obligation }
    }

    /// Resolve `obligation` — explicit only (see [`Resolution`]).
    pub fn resolve(&self, obligation: &str) -> Resolution<'a> {
        match self.by_obligation.get(obligation) {
            None => Resolution::Undispositioned,
            Some(records) if records.len() == 1 => Resolution::One(records[0]),
            Some(records) => Resolution::Conflicting(records.clone()),
        }
    }

    /// Every `(obligation id, records)` entry, obligation-id sorted.
    pub fn entries(&self) -> impl Iterator<Item = (&'a str, &[&'a ObligationDisposition])> {
        self.by_obligation
            .iter()
            .map(|(id, records)| (*id, records.as_slice()))
    }
}

/// The default `odp-` id for a machine-emitted skeleton: the obligation's own tail under
/// the `odp-` prefix (`obl-cmb-3f9a2c17` → `odp-cmb-3f9a2c17`).
///
/// A convention, **not** a rule: unlike `cls-`/`clc-`, whose id must mirror its unit's
/// tail (V13), an `odp-` id may be any well-formed slug — data-model.md §6's own example
/// is the semantic `odp-up-compose-lockfile`. Mirroring is simply what a generator can
/// pick without inventing meaning, and it makes a bulk-authored record traceable to its
/// obligation by eye. `obligation` is the load-bearing link either way.
pub fn scaffold_disposition_id(obligation: &str) -> String {
    match obligation.strip_prefix("obl-") {
        Some(tail) => format!("odp-{tail}"),
        None => format!("odp-{obligation}"),
    }
}

/// The join of a generated obligation inventory against a registry's hand-authored
/// `odp-` records — the raw material behind V28 and V29's staleness arm, and the
/// population `coverage scaffold` emits.
///
/// Extracted for the reason [`crate::validate::join_inventory`] was: `validate`'s
/// enforcement, `certify`'s blocking set, and the scaffold's worklist must be computed by
/// ONE implementation. If the scaffold's idea of "undispositioned" could drift from
/// V28's, the workflow would loop forever — scaffold, fill in everything it emitted,
/// still fail.
#[derive(Debug, Clone, Default)]
pub struct DispositionAudit<'a> {
    /// Applicable obligations carrying **zero** records (V28), id-sorted.
    pub undispositioned: Vec<&'a Obligation>,
    /// Obligations carrying **more than one** record (V28), id-sorted; the records under
    /// each are id-sorted too.
    pub conflicting: Vec<(&'a Obligation, Vec<&'a ObligationDisposition>)>,
    /// Records whose `obligation` resolves to nothing (V29 stale), id-sorted.
    pub stale: Vec<&'a ObligationDisposition>,
    /// Cleanly resolved `(obligation, record)` pairs, obligation-id sorted.
    pub resolved: Vec<(&'a Obligation, &'a ObligationDisposition)>,
    /// Obligations excluded from the applicable population because their environment is
    /// not the active one, id-sorted. Reported, never blocking, never "covered".
    pub inactive: Vec<&'a Obligation>,
}

/// Join `inventory` against `registry`'s `odp-` records (see [`DispositionAudit`]).
///
/// `inactive` is derived from the active profile — a behavior obligation whose behavior
/// falls outside it. It is computed here, and not left to each caller, so that "which
/// obligations must be dispositioned" cannot mean one thing to `validate` and another to
/// `certify`. Combination obligations are never inactive: scenario dimensions describe
/// what an operation exercises, not where it can run (research Decision 1).
pub fn audit_dispositions<'a>(
    registry: &'a Registry,
    inventory: &'a ObligationInventory,
) -> DispositionAudit<'a> {
    use std::collections::BTreeSet;

    let index = DispositionIndex::build(registry);
    let coverage = crate::coverage::Coverage::evaluate(registry);
    let out_of_profile: BTreeSet<&str> = coverage
        .out_of_profile
        .iter()
        .map(|b| b.id.as_str())
        .collect();

    let mut audit = DispositionAudit::default();
    let known: BTreeSet<&str> = inventory.units.iter().map(|u| u.id.as_str()).collect();

    for unit in &inventory.units {
        let inactive = unit.kind == ObligationKind::Behavior
            && unit
                .behavior
                .as_deref()
                .is_some_and(|b| out_of_profile.contains(b));
        if inactive {
            audit.inactive.push(unit);
        }
        match index.resolve(&unit.id) {
            Resolution::One(record) => audit.resolved.push((unit, record)),
            Resolution::Conflicting(records) => audit.conflicting.push((unit, records)),
            // An inactive obligation is NOT applicable, so nobody owes it a decision. It
            // stays enumerated (in `inactive`) so it reads as backlog rather than
            // disappearing from the denominator — the arithmetic FR-004a forbids.
            Resolution::Undispositioned if inactive => {}
            Resolution::Undispositioned => audit.undispositioned.push(unit),
        }
    }

    for record in &registry.obligation_dispositions {
        if !known.contains(record.obligation.as_str()) {
            audit.stale.push(record);
        }
    }

    audit.undispositioned.sort_by(|a, b| a.id.cmp(&b.id));
    audit.conflicting.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    audit.stale.sort_by(|a, b| a.id.cmp(&b.id));
    audit.resolved.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    audit.inactive.sort_by(|a, b| a.id.cmp(&b.id));
    audit
}

/// A one-line human description of what an obligation is about, for a diagnostic that has
/// to be actionable without opening the machine-owned inventory (FR-024, scenario 1: the
/// message names the obligation, its behavior or operation+assignment, and its context).
pub fn describe(unit: &Obligation) -> String {
    match unit.kind {
        ObligationKind::Behavior => {
            let behavior = unit.behavior.as_deref().unwrap_or("<no behavior>");
            let context = unit.context.as_deref().unwrap_or(&[]);
            if context.is_empty() {
                format!("behavior {behavior} (context: any)")
            } else {
                let rendered: Vec<String> = context
                    .iter()
                    .map(|c| format!("{}={}", c.dimension, c.values.join("|")))
                    .collect();
                format!("behavior {behavior} (context: {})", rendered.join(", "))
            }
        }
        ObligationKind::Combination => {
            let operation = unit.operation.as_deref().unwrap_or("<no operation>");
            let assignment = unit
                .assignment
                .as_ref()
                .map(|a| {
                    a.iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            format!("operation {operation} (assignment: {assignment})")
        }
    }
}

// ---------------------------------------------------------------------------
// Serialization + atomic write
// ---------------------------------------------------------------------------

/// Render the inventory to its canonical string form: 2-space indent, LF endings,
/// trailing newline, no timestamps and no absolute paths. Identical inputs render
/// byte-identically on every platform (SC-010), which is what makes `coverage check`'s
/// byte comparison meaningful.
pub fn render(inventory: &ObligationInventory) -> String {
    let mut out = serde_json::to_string_pretty(inventory)
        .unwrap_or_else(|e| unreachable!("obligation serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Atomically write the rendered inventory to `path`, delegating to the single
/// [`crate::atomic_write`] primitive (temp file + rename). Never leaves a partial file.
pub fn write_obligations(
    path: &std::path::Path,
    inventory: &ObligationInventory,
) -> std::io::Result<()> {
    crate::atomic_write(path, &render(inventory))
}

/// A compact drift summary between a committed inventory and a fresh regeneration — the
/// `coverage check` mismatch report (contracts/coverage-cli.md).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObligationDrift {
    /// Ids present in the regeneration but not in the committed file, id-sorted.
    pub added: Vec<String>,
    /// Ids present in the committed file but not in the regeneration, id-sorted.
    pub removed: Vec<String>,
    /// Ids present in both whose record content differs, id-sorted.
    pub changed: Vec<String>,
}

impl ObligationDrift {
    /// Whether the two inventories are unit-identical.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// The first differing unit id and how it differs — the message
    /// contracts/coverage-cli.md requires `coverage check` to print on drift.
    ///
    /// "First" is by id across all three buckets, so the answer does not depend on which
    /// bucket happens to be inspected first.
    pub fn first_difference(&self) -> Option<(String, &'static str)> {
        let mut all: Vec<(&String, &'static str)> = Vec::new();
        all.extend(self.added.iter().map(|id| (id, "added")));
        all.extend(self.removed.iter().map(|id| (id, "removed")));
        all.extend(self.changed.iter().map(|id| (id, "changed")));
        all.sort();
        all.first().map(|(id, how)| ((*id).clone(), *how))
    }
}

/// Compare a committed inventory against a fresh regeneration, matched by obligation id.
///
/// Id matching is the right key precisely *because* the id is substance-anchored: a unit
/// whose substance changed gets a new id, so it reads as an add/remove pair, which is
/// what it is — a new obligation needing its own decision. `changed` therefore captures
/// only non-identity drift (a field the hash does not cover).
pub fn compare(
    committed: &ObligationInventory,
    regenerated: &ObligationInventory,
) -> ObligationDrift {
    use std::collections::BTreeMap;
    let old: BTreeMap<&str, &Obligation> =
        committed.units.iter().map(|u| (u.id.as_str(), u)).collect();
    let new: BTreeMap<&str, &Obligation> = regenerated
        .units
        .iter()
        .map(|u| (u.id.as_str(), u))
        .collect();

    let mut drift = ObligationDrift::default();
    for (id, unit) in &new {
        match old.get(id) {
            None => drift.added.push((*id).to_string()),
            Some(previous) if previous != unit => drift.changed.push((*id).to_string()),
            Some(_) => {}
        }
    }
    for id in old.keys() {
        if !new.contains_key(id) {
            drift.removed.push((*id).to_string());
        }
    }
    drift
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use indexmap::IndexMap;

    use super::*;

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn condition(dimension: &str, values: &[&str]) -> Condition {
        Condition {
            dimension: dimension.to_string(),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    /// The context encoding must be INJECTIVE across condition arity. With a single
    /// separator everywhere, `[{a:[b]}, {c:[d]}]` and `[{a:[b,c,d]}]` both rendered
    /// `a␟b␟c␟d␟` — two distinct behavior obligations collapsing onto one id, silently
    /// transferring one behavior's hand-authored `odp-` disposition to the other.
    #[test]
    fn context_encoding_does_not_collide_across_arity() {
        let split = [condition("a", &["b"]), condition("c", &["d"])];
        let joined = [condition("a", &["b", "c", "d"])];
        assert_ne!(canonical_context(&split), canonical_context(&joined));
        assert_ne!(
            behavior_obligation_id("bhv-x", &split),
            behavior_obligation_id("bhv-x", &joined)
        );
    }

    /// T013: the id must not depend on the order the author happened to write the pairs
    /// in. Built from an `IndexMap` — which PRESERVES insertion order, so a function
    /// that failed to sort internally would visibly fork here — rather than from a
    /// `BTreeMap`, which would sort on the caller's behalf and make the test vacuous.
    #[test]
    fn assignment_key_order_does_not_change_the_combination_id() {
        let mut forward: IndexMap<String, String> = IndexMap::new();
        forward.insert("sdim-config-source".to_string(), "compose".to_string());
        forward.insert("sdim-features".to_string(), "lockfile".to_string());
        forward.insert("sdim-layering".to_string(), "single".to_string());

        let mut reverse: IndexMap<String, String> = IndexMap::new();
        reverse.insert("sdim-layering".to_string(), "single".to_string());
        reverse.insert("sdim-features".to_string(), "lockfile".to_string());
        reverse.insert("sdim-config-source".to_string(), "compose".to_string());

        assert_ne!(
            forward.keys().collect::<Vec<_>>(),
            reverse.keys().collect::<Vec<_>>(),
            "the fixture must actually differ in iteration order, or the test proves nothing"
        );
        assert_eq!(
            combination_obligation_id("up", &forward),
            combination_obligation_id("up", &reverse),
            "assignment key order must not change obl-cmb identity (contracts/obligation.md)"
        );

        // The same claim through two other container shapes, so the guarantee is a
        // property of the function and not of one caller's choice of map.
        let vector = pairs(&[
            ("sdim-layering", "single"),
            ("sdim-config-source", "compose"),
            ("sdim-features", "lockfile"),
        ]);
        let tree: BTreeMap<String, String> = vector.iter().cloned().collect();
        assert_eq!(
            combination_obligation_id("up", &forward),
            combination_obligation_id("up", vector.clone()),
            "a Vec in a third order must agree with the IndexMap"
        );
        assert_eq!(
            combination_obligation_id("up", &forward),
            combination_obligation_id("up", &tree),
            "a BTreeMap must agree with the IndexMap"
        );
    }

    #[test]
    fn combination_id_tracks_operation_and_assignment_substance() {
        let base = pairs(&[
            ("sdim-config-source", "compose"),
            ("sdim-features", "lockfile"),
        ]);
        let id = combination_obligation_id("up", base.clone());
        assert!(id.starts_with("obl-cmb-"), "id must be prefixed: {id}");
        assert_eq!(id.len(), "obl-cmb-".len() + 8, "hash8 is 8 hex chars: {id}");

        assert_ne!(
            id,
            combination_obligation_id("build", base.clone()),
            "the same pair under a different operation is a different obligation"
        );
        assert_ne!(
            id,
            combination_obligation_id(
                "up",
                pairs(&[
                    ("sdim-config-source", "image"),
                    ("sdim-features", "lockfile"),
                ]),
            ),
            "changing a value is a different obligation"
        );
        assert_ne!(
            id,
            combination_obligation_id(
                "up",
                pairs(&[
                    ("sdim-config-source", "compose"),
                    ("sdim-features", "lockfile"),
                    ("sdim-container-state", "running"),
                ]),
            ),
            "a triple is not the pair it contains"
        );
        assert_eq!(
            id,
            combination_obligation_id("up", base),
            "identical inputs are identical ids"
        );
    }

    /// The separator must keep the concatenation injective: two different splits of the
    /// same characters must not collide.
    #[test]
    fn combination_id_separator_prevents_field_smearing() {
        assert_ne!(
            combination_obligation_id("up", pairs(&[("ab", "c")])),
            combination_obligation_id("up", pairs(&[("a", "bc")])),
            "a key/value split must be visible to the hash"
        );
        assert_ne!(
            combination_obligation_id("up", pairs(&[("a", "b"), ("c", "d")])),
            combination_obligation_id("up", pairs(&[("a", "bcd")])),
            "a pair boundary must be visible to the hash"
        );
    }

    #[test]
    fn behavior_id_is_stable_under_cosmetic_context_reordering() {
        let forward = vec![
            condition("dim-runtime", &["docker", "podman"]),
            condition("dim-os", &["linux"]),
        ];
        // The same context: conditions swapped, and one condition's value SUBSET
        // written in the other order.
        let reordered = vec![
            condition("dim-os", &["linux"]),
            condition("dim-runtime", &["podman", "docker"]),
        ];
        assert_eq!(
            behavior_obligation_id("bhv-x", &forward),
            behavior_obligation_id("bhv-x", &reordered),
            "condition order and value order are both insignificant"
        );
    }

    #[test]
    fn behavior_id_tracks_behavior_and_context_substance() {
        let context = vec![condition("dim-runtime", &["docker"])];
        let id = behavior_obligation_id("bhv-x", &context);
        assert!(id.starts_with("obl-bhv-"), "id must be prefixed: {id}");
        assert_eq!(id.len(), "obl-bhv-".len() + 8, "hash8 is 8 hex chars: {id}");

        assert_ne!(
            id,
            behavior_obligation_id("bhv-y", &context),
            "a different behavior is a different obligation"
        );
        assert_ne!(
            id,
            behavior_obligation_id("bhv-x", &[condition("dim-runtime", &["podman"])]),
            "a different pinned value is a different obligation"
        );
        assert_ne!(
            id,
            behavior_obligation_id("bhv-x", &[condition("dim-runtime", &["docker", "podman"])]),
            "widening the value subset is a different obligation"
        );
        assert_ne!(
            id,
            behavior_obligation_id("bhv-x", &[]),
            "an empty context (applies everywhere) is a different obligation"
        );
    }

    /// The two kinds share a truncation convention but MUST NOT share an id space: a
    /// behavior obligation and a combination obligation are different decisions.
    #[test]
    fn the_two_kinds_are_distinguishable_by_prefix() {
        let bhv = behavior_obligation_id("x", &[]);
        let cmb = combination_obligation_id("x", pairs(&[]));
        assert!(bhv.starts_with("obl-bhv-"));
        assert!(cmb.starts_with("obl-cmb-"));
        assert_ne!(
            bhv, cmb,
            "the two kinds must not collide on identical input"
        );
    }

    // -- Generation (T037/T038) ------------------------------------------------

    use crate::model::{
        BehaviorUnit, Decision, ReferenceStatus, RevisionKind, SourceRevision, SpecStatus,
    };
    use crate::scenario::{
        ApplicabilityRule, HighRiskTriple, ScenarioDimension, ScenarioDimensionKind,
    };

    fn dimension(id: &str, values: &[&str]) -> ScenarioDimension {
        ScenarioDimension {
            id: id.to_string(),
            kind: ScenarioDimensionKind::Scenario,
            description: format!("test dimension {id}"),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn behavior(id: &str, applicability: Vec<Condition>) -> BehaviorUnit {
        BehaviorUnit {
            id: id.to_string(),
            area: "test".to_string(),
            statement: format!("statement for {id}"),
            applicability,
            spec: SpecStatus::Conformant,
            reference: ReferenceStatus::Aligned,
            decision: Decision::FollowSpec,
            notes: None,
        }
    }

    /// A minimal registry: two operations, two pairable dimensions, one exclusion rule.
    fn registry_fixture() -> Registry {
        let mut registry = Registry {
            revisions: vec![SourceRevision {
                id: "rev-spec-abcdef".to_string(),
                kind: RevisionKind::Spec,
                pin: "abcdef".to_string(),
                url: "https://example.invalid".to_string(),
                verified_against: None,
            }],
            ..Registry::default()
        };
        registry.scenario = vec![
            dimension(OPERATION_DIMENSION, &["read-configuration", "up"]),
            dimension("sdim-container-state", &["none", "running"]),
            dimension("sdim-output-mode", &["structured", "human"]),
        ];
        registry.applicability = vec![ApplicabilityRule {
            id: "rule-no-container-state-without-a-container".to_string(),
            excludes: vec![
                Condition {
                    dimension: OPERATION_DIMENSION.to_string(),
                    values: vec!["read-configuration".to_string()],
                },
                Condition {
                    dimension: "sdim-container-state".to_string(),
                    values: vec!["running".to_string()],
                },
            ],
            ground: "read-configuration never creates or inspects a container, so a running \
                     container is not a property it can exercise"
                .to_string(),
        }];
        registry
    }

    #[test]
    fn pairs_are_partitioned_by_operation_and_pruned_by_rules() {
        let registry = registry_fixture();
        let inventory = generate_obligations(&registry).expect("generate");

        let of = |operation: &str| -> Vec<Vec<(String, String)>> {
            inventory
                .units
                .iter()
                .filter(|u| u.operation.as_deref() == Some(operation))
                .map(|u| {
                    u.assignment
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .collect()
        };

        // `up`: 2 × 2 = 4 pairs; `read-configuration` loses the `running` value → 1 × 2.
        assert_eq!(of("up").len(), 4);
        assert_eq!(of("read-configuration").len(), 2);
        assert!(
            of("read-configuration")
                .iter()
                .all(|pairs| pairs.iter().all(|(_, v)| v != "running")),
            "a value excluded under an operation must not appear in its pairs"
        );

        // The SAME pair under two operations is two obligations (FR-013a): coverage under
        // one operation must never mask another's.
        let up = combination_obligation_id(
            "up",
            pairs(&[
                ("sdim-container-state", "none"),
                ("sdim-output-mode", "human"),
            ]),
        );
        let read = combination_obligation_id(
            "read-configuration",
            pairs(&[
                ("sdim-container-state", "none"),
                ("sdim-output-mode", "human"),
            ]),
        );
        assert_ne!(up, read);
        let ids: Vec<&str> = inventory.units.iter().map(|u| u.id.as_str()).collect();
        assert!(ids.contains(&up.as_str()) && ids.contains(&read.as_str()));

        // The operation dimension is a partition key, never a pair member.
        for unit in &inventory.units {
            if let Some(assignment) = unit.assignment.as_ref() {
                assert!(!assignment.contains_key(OPERATION_DIMENSION), "{}", unit.id);
            }
        }
    }

    #[test]
    fn a_model_with_no_operation_dimension_emits_no_combination_obligations() {
        let mut registry = registry_fixture();
        registry.scenario.retain(|d| d.id != OPERATION_DIMENSION);
        let inventory = generate_obligations(&registry).expect("generate");
        assert!(
            inventory
                .units
                .iter()
                .all(|u| u.kind == ObligationKind::Behavior),
            "without a partition key, emitting pairs would fabricate a denominator"
        );
    }

    #[test]
    fn each_behavior_yields_exactly_one_obligation_carrying_its_applicability() {
        let mut registry = registry_fixture();
        registry.behaviors = vec![
            behavior("bhv-everywhere", vec![]),
            behavior(
                "bhv-podman-only",
                vec![Condition {
                    dimension: "dim-runtime".to_string(),
                    values: vec!["podman".to_string(), "docker".to_string()],
                }],
            ),
        ];
        let inventory = generate_obligations(&registry).expect("generate");
        let behaviors: Vec<&Obligation> = inventory
            .units
            .iter()
            .filter(|u| u.kind == ObligationKind::Behavior)
            .collect();

        assert_eq!(
            behaviors.len(),
            2,
            "one obligation per behavior — never zero (which would erase it from the \
             denominator) and never one per value (which would multiply the two kinds)"
        );
        let everywhere = behaviors
            .iter()
            .find(|u| u.behavior.as_deref() == Some("bhv-everywhere"))
            .unwrap();
        assert_eq!(
            everywhere.context.as_deref(),
            Some(&[][..]),
            "an empty applicability is one universal context, serialized as []"
        );
        let podman = behaviors
            .iter()
            .find(|u| u.behavior.as_deref() == Some("bhv-podman-only"))
            .unwrap();
        assert_eq!(
            podman.context.as_ref().map(Vec::len),
            Some(1),
            "a value subset means \"any of these\", so it stays ONE condition"
        );
    }

    #[test]
    fn a_high_risk_triple_becomes_an_arity_three_obligation() {
        let mut registry = registry_fixture();
        let mut assignment = IndexMap::new();
        assignment.insert(OPERATION_DIMENSION.to_string(), "up".to_string());
        assignment.insert("sdim-container-state".to_string(), "running".to_string());
        assignment.insert("sdim-output-mode".to_string(), "structured".to_string());
        registry.triples = vec![HighRiskTriple {
            id: "hrt-test".to_string(),
            assignment,
            reason: "a selected interaction whose reason exists so the selection is reviewable"
                .to_string(),
        }];

        let inventory = generate_obligations(&registry).expect("generate");
        let triple = inventory
            .units
            .iter()
            .find(|u| u.is_triple())
            .expect("the triple is emitted");
        assert_eq!(triple.operation.as_deref(), Some("up"));
        assert_eq!(triple.assignment.as_ref().map(IndexMap::len), Some(2));
        assert!(
            !triple
                .assignment
                .as_ref()
                .unwrap()
                .contains_key(OPERATION_DIMENSION),
            "the operation is split out, so a triple and a pair share one identity scheme"
        );
    }

    #[test]
    fn generation_is_id_sorted_and_pins_the_spec_revision() {
        let registry = registry_fixture();
        let inventory = generate_obligations(&registry).expect("generate");
        let ids: Vec<&str> = inventory.units.iter().map(|u| u.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "step 11 sorts by id");
        assert_eq!(inventory.revision, "rev-spec-abcdef");
        assert_eq!(inventory.schema_version, OBLIGATION_SCHEMA_VERSION);

        let rendered = render(&inventory);
        assert!(
            rendered.ends_with('\n'),
            "canonical form is newline-terminated"
        );
        assert_eq!(rendered, render(&generate_obligations(&registry).unwrap()));
    }

    #[test]
    fn generation_without_a_spec_revision_is_refused_rather_than_guessed() {
        let mut registry = registry_fixture();
        registry.revisions.clear();
        assert_eq!(
            generate_obligations(&registry).unwrap_err(),
            ObligationError::NoSpecRevision,
            "an inventory with a guessed pin would have no provenance at all"
        );
    }

    #[test]
    fn drift_names_the_first_differing_unit() {
        let registry = registry_fixture();
        let regenerated = generate_obligations(&registry).expect("generate");
        let mut committed = regenerated.clone();
        let removed = committed.units.remove(0);

        let drift = compare(&committed, &regenerated);
        assert_eq!(drift.added, vec![removed.id.clone()]);
        assert!(drift.removed.is_empty() && drift.changed.is_empty());
        assert_eq!(drift.first_difference(), Some((removed.id, "added")));
        assert!(compare(&regenerated, &regenerated).is_empty());
    }

    // -----------------------------------------------------------------------
    // Dispositions (T061–T063)
    // -----------------------------------------------------------------------

    fn parse_disposition(json: &str) -> Result<ObligationDisposition, String> {
        serde_json::from_str::<ObligationDisposition>(json).map_err(|e| e.to_string())
    }

    /// Each of the four dispositions loads with exactly its own payload, and with the
    /// other three absent — the positive control the rejection tests need.
    #[test]
    fn each_disposition_loads_with_exactly_its_own_payload() {
        let case = parse_disposition(
            r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa",
                 "disposition": "case", "cases": ["case-x"] }"#,
        )
        .expect("case disposition loads");
        assert_eq!(case.disposition, DispositionKind::Case);
        assert_eq!(case.cases, vec!["case-x".to_string()]);

        let non_testable = parse_disposition(
            r#"{ "id": "odp-b", "obligation": "obl-cmb-0000bbbb",
                 "disposition": "non-testable", "rationale": "no observable channel" }"#,
        )
        .expect("non-testable disposition loads");
        assert_eq!(non_testable.disposition, DispositionKind::NonTestable);

        let waived = parse_disposition(
            r#"{ "id": "odp-c", "obligation": "obl-cmb-0000cccc",
                 "disposition": "waived", "waiver": "wvr-x" }"#,
        )
        .expect("waived disposition loads");
        assert_eq!(waived.waiver.as_deref(), Some("wvr-x"));

        let gap = parse_disposition(
            r#"{ "id": "odp-d", "obligation": "obl-cmb-0000dddd",
                 "disposition": "gap", "gap": "gap-x" }"#,
        )
        .expect("gap disposition loads");
        assert_eq!(gap.gap.as_deref(), Some("gap-x"));
    }

    /// A payload that disagrees with the word — too many, too few, or the wrong one — is
    /// refused at DESERIALIZE time, so a half-stated judgement never reaches validation.
    #[test]
    fn a_payload_disagreeing_with_the_disposition_is_refused_at_load() {
        // Two payloads.
        let both = parse_disposition(
            r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa", "disposition": "case",
                 "cases": ["case-x"], "gap": "gap-y" }"#,
        )
        .expect_err("two payloads must be refused");
        assert!(
            both.contains("cases, gap"),
            "message must list both: {both}"
        );

        // No payload at all.
        let none = parse_disposition(
            r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa", "disposition": "waived" }"#,
        )
        .expect_err("a waived disposition with no waiver must be refused");
        assert!(none.contains("nothing"), "message must say so: {none}");

        // The wrong payload for the word.
        let wrong = parse_disposition(
            r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa",
                 "disposition": "non-testable", "waiver": "wvr-x" }"#,
        )
        .expect_err("a non-testable disposition backed by a waiver must be refused");
        assert!(
            wrong.contains("rationale"),
            "message names what it needs: {wrong}"
        );

        // An unknown disposition word.
        assert!(
            parse_disposition(
                r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa",
                     "disposition": "probably-fine", "rationale": "r" }"#,
            )
            .is_err(),
            "the disposition vocabulary is closed"
        );
    }

    /// A blank payload, and the `UNREVIEWED` scaffold sentinel, are both refused (T070).
    #[test]
    fn a_blank_or_sentinel_payload_is_refused_at_load() {
        let blank = parse_disposition(
            r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa",
                 "disposition": "non-testable", "rationale": "   " }"#,
        )
        .expect_err("a blank rationale must be refused");
        assert!(blank.contains("blank"), "{blank}");

        let sentinel = parse_disposition(
            r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa",
                 "disposition": "non-testable", "rationale": "UNREVIEWED" }"#,
        )
        .expect_err("a scaffolded rationale must be refused");
        assert!(sentinel.contains("UNREVIEWED"), "{sentinel}");

        // And the scaffold's own `disposition` sentinel is refused by the closed enum,
        // so a skeleton is unloadable on both axes.
        assert!(
            parse_disposition(
                r#"{ "id": "odp-a", "obligation": "obl-cmb-0000aaaa",
                     "disposition": "UNREVIEWED", "rationale": "UNREVIEWED" }"#,
            )
            .is_err()
        );
    }

    fn disposition(id: &str, obligation: &str) -> ObligationDisposition {
        ObligationDisposition {
            id: id.to_string(),
            obligation: obligation.to_string(),
            disposition: DispositionKind::Gap,
            cases: Vec::new(),
            rationale: None,
            waiver: None,
            gap: Some("gap-x".to_string()),
        }
    }

    /// Resolution is explicit only: zero records is `Undispositioned` (never a default),
    /// and several is `Conflicting` (never a silently-picked winner).
    #[test]
    fn resolution_is_explicit_with_no_default_and_no_winner_picked_from_a_conflict() {
        let registry = Registry {
            obligation_dispositions: vec![
                disposition("odp-second", "obl-cmb-0000aaaa"),
                disposition("odp-first", "obl-cmb-0000aaaa"),
                disposition("odp-solo", "obl-cmb-0000bbbb"),
            ],
            ..Default::default()
        };
        let index = DispositionIndex::build(&registry);

        assert_eq!(
            index.resolve("obl-cmb-0000cccc"),
            Resolution::Undispositioned,
            "an obligation nobody judged has no disposition — not an implicit one"
        );
        assert_eq!(
            index
                .resolve("obl-cmb-0000bbbb")
                .single()
                .map(|r| r.id.as_str()),
            Some("odp-solo")
        );
        match index.resolve("obl-cmb-0000aaaa") {
            Resolution::Conflicting(records) => {
                assert_eq!(
                    records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
                    vec!["odp-first", "odp-second"],
                    "conflicting records are id-sorted so the report is stable"
                );
            }
            other => panic!("two records must conflict, got {other:?}"),
        }
        assert!(
            index.resolve("obl-cmb-0000aaaa").single().is_none(),
            "a conflict never resolves to one of its members"
        );
    }

    /// The audit separates the four populations, and an inactive-environment obligation is
    /// enumerated rather than counted as owing a decision.
    #[test]
    fn the_audit_separates_undispositioned_conflicting_stale_and_inactive() {
        let mut registry = registry_fixture();
        let inventory = generate_obligations(&registry).expect("generate");
        let first = inventory.units[0].id.clone();
        let second = inventory.units[1].id.clone();

        registry.obligation_dispositions = vec![
            disposition("odp-one", &first),
            disposition("odp-two", &second),
            disposition("odp-three", &second),
            disposition("odp-ghost", "obl-cmb-deadbeef"),
        ];

        let audit = audit_dispositions(&registry, &inventory);
        assert_eq!(
            audit
                .resolved
                .iter()
                .map(|(u, _)| u.id.as_str())
                .collect::<Vec<_>>(),
            vec![first.as_str()]
        );
        assert_eq!(
            audit
                .conflicting
                .iter()
                .map(|(u, _)| u.id.as_str())
                .collect::<Vec<_>>(),
            vec![second.as_str()]
        );
        assert_eq!(
            audit
                .stale
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            vec!["odp-ghost"],
            "a record naming no live obligation is stale, not silently dropped"
        );
        assert_eq!(
            audit.undispositioned.len(),
            inventory.units.len() - 2,
            "every other obligation still owes a decision"
        );
        assert!(
            audit.undispositioned.windows(2).all(|w| w[0].id <= w[1].id),
            "the queue is id-sorted"
        );
    }

    /// A scaffold id mirrors its obligation's tail, and the two prefixes stay disjoint.
    #[test]
    fn a_scaffold_id_mirrors_its_obligations_tail() {
        assert_eq!(
            scaffold_disposition_id("obl-cmb-3f9a2c17"),
            "odp-cmb-3f9a2c17"
        );
        assert_eq!(
            scaffold_disposition_id("obl-bhv-079ac919"),
            "odp-bhv-079ac919"
        );
        assert!(scaffold_disposition_id("obl-cmb-3f9a2c17").starts_with("odp-"));
    }
}
