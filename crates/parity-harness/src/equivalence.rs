//! The equivalence ledger — `target/parity/equivalence.json` (data-model.md §5).
//!
//! Per-unit outcome comparison between a superseded carrier and its replacement. It is
//! the evidence that permits deleting a hand-written parity program: a carrier is
//! deletable **iff** every unit it carries relates as `equivalent` or `stricter`
//! **and** no residual record names it as `blockedCarrier` (FR-035, FR-038).
//!
//! # The relation is decided on outcome, not message text (spec A-002)
//!
//! Two paths that both fail are equivalent even when they word the failure differently;
//! two paths that both pass are equivalent even when one prints more detail. What
//! matters is whether a difference was *detected*:
//!
//! - identical outcomes → [`Relation::Equivalent`];
//! - a difference detected **only by the replacement** → [`Relation::Stricter`] —
//!   permitted, but reported and acted on: the newly detected difference must be
//!   characterized as a case or a waiver, never suppressed (FR-036);
//! - a difference detected **only by the legacy path** → [`Relation::MorePermissive`],
//!   which **blocks deletion** (FR-035). This is the single condition the whole
//!   migration exists to prevent silently occurring.
//!
//! This module defines the ledger's record shapes. Relation classification, the
//! deletion predicate, and the live `equivalence-report` producer land with User
//! Story 7 (T080–T082); the producer requires the verified pinned oracle and Docker and
//! fails loud when either is absent — it never skips to a pass (constitution IV).

use serde::{Deserialize, Serialize};

/// How the replacement's outcome relates to the superseded path's outcome for one
/// baseline unit (data-model.md §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    /// Identical outcomes — the replacement neither missed nor invented a difference.
    Equivalent,
    /// A difference detected only by the replacement. Permitted and reported; the new
    /// difference must be characterized (FR-036).
    Stricter,
    /// A difference detected only by the legacy path. **Blocks deletion** (FR-035).
    MorePermissive,
}

impl Relation {
    /// Whether this relation permits deleting the carrier, all other conditions being
    /// met. Only `more-permissive` blocks.
    pub fn permits_deletion(self) -> bool {
        !matches!(self, Relation::MorePermissive)
    }

    /// Whether this relation requires a non-empty `detail`. Both non-`equivalent`
    /// relations do: a reported difference with no explanation is unactionable.
    pub fn requires_detail(self) -> bool {
        !matches!(self, Relation::Equivalent)
    }

    /// The wire spelling, for diagnostics that name the unsatisfied condition.
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Equivalent => "equivalent",
            Relation::Stricter => "stricter",
            Relation::MorePermissive => "more-permissive",
        }
    }
}

/// The comparison outcome of ONE path over one unit, reduced to the three states a
/// relation is computed from (spec A-002: outcome, never message text).
///
/// Reducing both paths to this vocabulary BEFORE comparing is what makes the relation
/// message-independent by construction rather than by discipline: two paths that word a
/// failure differently, or order their findings differently, cannot produce different
/// relations because the wording never reaches the classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComparisonOutcome {
    /// No difference was reported — a pass, or a difference fully covered by a
    /// characterized exception (a decision already made, not a difference left
    /// undetected).
    Clean,
    /// A difference was reported.
    Difference,
    /// The comparison did not complete (oracle failure, timeout, malformed output,
    /// normalization failure, a stale snapshot, no reference for this platform). NOT a
    /// pass and NOT a difference — a run that did not happen cannot support a relation.
    Error,
}

impl ComparisonOutcome {
    /// Reduce a reported outcome name to its comparison state.
    ///
    /// Covers both vocabularies — the legacy `report::Outcome` spellings and the
    /// declarative `evidence::Outcome` spellings — so ONE classifier serves both paths.
    /// An unrecognized spelling is [`ComparisonOutcome::Error`]: fail-closed, because a
    /// state we cannot interpret must never be read as agreement.
    pub fn from_outcome_name(name: &str) -> ComparisonOutcome {
        match name {
            // Legacy: a clean pass, or one covered by a matching waiver.
            "pass" | "pass-waived" => ComparisonOutcome::Clean,
            // Declarative: agreement, or a divergence covered by a scoped tolerance.
            "agree" | "allowed-difference" => ComparisonOutcome::Clean,
            // Both vocabularies: a reported difference.
            "diverge" | "divergence" | "fail" => ComparisonOutcome::Difference,
            _ => ComparisonOutcome::Error,
        }
    }

    /// Whether this outcome supports a relation at all.
    pub fn is_classifiable(self) -> bool {
        !matches!(self, ComparisonOutcome::Error)
    }
}

/// Classify the relation between the two paths' outcomes (spec A-002).
///
/// `None` when either side did not complete: an unproven unit is NOT an equivalent one,
/// and reading "we could not check" as "it is fine" is precisely how a more-permissive
/// replacement gets deleted into.
pub fn classify_relation(
    legacy: ComparisonOutcome,
    replacement: ComparisonOutcome,
) -> Option<Relation> {
    match (legacy, replacement) {
        (ComparisonOutcome::Error, _) | (_, ComparisonOutcome::Error) => None,
        // The same outcome either way: the replacement neither missed nor invented a
        // difference. Both-clean and both-difference are equally equivalent — what
        // matters is whether a difference was DETECTED, not how loudly.
        (a, b) if a == b => Some(Relation::Equivalent),
        (ComparisonOutcome::Clean, ComparisonOutcome::Difference) => Some(Relation::Stricter),
        (ComparisonOutcome::Difference, ComparisonOutcome::Clean) => Some(Relation::MorePermissive),
        // Unreachable: the arms above are exhaustive over {Clean, Difference}².
        _ => None,
    }
}

/// One unit's entry in the ledger (data-model.md §5, contracts/equivalence-ledger.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquivalenceEntry {
    /// The baseline unit id (`<program>::<case-id>`).
    pub unit: String,
    /// The carrier the unit belongs to — the program deletion is gated on.
    pub carrier: String,
    /// Outcome under the superseded path.
    pub legacy_outcome: String,
    /// Outcome under the authoritative declarative runner.
    pub replacement_outcome: String,
    /// How the two relate — decided on outcome, never on message text.
    pub relation: Relation,
    /// Required for `stricter` and `more-permissive`; names the difference that one
    /// path saw and the other did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Required for `stricter`: the `case-…` / `wvr-…` / issue reference characterizing
    /// the newly detected difference. An uncharacterized improvement is suppression, not
    /// an improvement (FR-036).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characterized_as: Option<String>,
}

impl EquivalenceEntry {
    /// The reasons this entry is malformed, if any (contracts/equivalence-ledger.md).
    ///
    /// `detail` is required whenever the two paths disagreed; `characterizedAs` is
    /// required for `stricter`.
    pub fn defects(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.relation.requires_detail()
            && self.detail.as_ref().is_none_or(|d| d.trim().is_empty())
        {
            out.push(format!(
                "`{}` is `{}` with no `detail` — a reported difference with no \
                 explanation is unactionable",
                self.unit,
                self.relation.as_str()
            ));
        }
        if self.relation == Relation::Stricter
            && self
                .characterized_as
                .as_ref()
                .is_none_or(|c| c.trim().is_empty())
        {
            out.push(format!(
                "`{}` is `stricter` with no `characterizedAs` — a newly detected \
                 difference must be characterized as a case, a waiver or a tracked \
                 issue, never suppressed (FR-036)",
                self.unit
            ));
        }
        out
    }
}

/// The `equivalence.json` envelope (derived, parity lane only — never committed).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquivalenceLedger {
    /// The baseline revision the ledger was computed against.
    #[serde(default)]
    pub baseline_revision: String,
    /// Per-unit entries, sorted by `unit` for deterministic output.
    #[serde(default)]
    pub entries: Vec<EquivalenceEntry>,
}

/// The verdict on one carrier's deletability (contracts/equivalence-ledger.md).
///
/// A blocked deletion NAMES the unsatisfied condition — which unit, which relation,
/// which residual — because "not deletable" without a reason is indistinguishable from
/// "nobody looked".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionVerdict {
    /// The carrier under judgement.
    pub carrier: String,
    /// True only when every condition holds.
    pub deletable: bool,
    /// One entry per unsatisfied condition, each naming the specific item.
    pub unsatisfied: Vec<String>,
}

/// Evaluate the deletion predicate for one carrier (FR-034, FR-037).
///
/// A carrier is deletable **iff** all four conditions hold:
///
/// 1. every unit it carries appears in the ledger;
/// 2. every such unit's relation is `equivalent` or `stricter`;
/// 3. no residual record names it as `blockedCarrier`;
/// 4. the coverage report accounts for every unit it carried.
///
/// `units` is every baseline unit belonging to the carrier; `ledger` the entries for it;
/// `blocking_residuals` the residual ids naming it; `unaccounted` the units the
/// conservation report could not account for (condition 4, reusing the report rather
/// than recomputing it — one accounting, not two).
pub fn deletion_verdict(
    carrier: &str,
    units: &[String],
    ledger: &[EquivalenceEntry],
    blocking_residuals: &[String],
    unaccounted: &[String],
) -> DeletionVerdict {
    let mut unsatisfied: Vec<String> = Vec::new();

    let judged: std::collections::BTreeMap<&str, &EquivalenceEntry> = ledger
        .iter()
        .filter(|e| e.carrier == carrier)
        .map(|e| (e.unit.as_str(), e))
        .collect();

    // Condition 1 + 2, per unit, naming the unit.
    for unit in units {
        match judged.get(unit.as_str()) {
            None => unsatisfied.push(format!(
                "condition 1: unit `{unit}` has no equivalence verdict — unproven is not \
                 the same as safe"
            )),
            Some(entry) if !entry.relation.permits_deletion() => unsatisfied.push(format!(
                "condition 2: unit `{unit}` is `{}` — the replacement misses a difference \
                 the superseded path catches{}",
                entry.relation.as_str(),
                entry
                    .detail
                    .as_deref()
                    .map(|d| format!(": {d}"))
                    .unwrap_or_default()
            )),
            Some(entry) => {
                // A malformed entry cannot support a deletion either.
                for defect in entry.defects() {
                    unsatisfied.push(format!("condition 2: {defect}"));
                }
            }
        }
    }

    // Condition 3, naming the residual.
    for residual in blocking_residuals {
        unsatisfied.push(format!(
            "condition 3: residual `{residual}` names `{carrier}` as its blocked carrier"
        ));
    }

    // Condition 4, naming the unit.
    let carried: std::collections::BTreeSet<&str> = units.iter().map(String::as_str).collect();
    for unit in unaccounted {
        if carried.contains(unit.as_str()) {
            unsatisfied.push(format!(
                "condition 4: unit `{unit}` is unaccounted for in the conservation report"
            ));
        }
    }

    // An empty carrier is not "trivially deletable" — it means the caller asked about a
    // program the baseline does not know, which is a wiring error, not a clearance.
    if units.is_empty() {
        unsatisfied.push(format!(
            "condition 1: carrier `{carrier}` carries no baseline unit — nothing has been \
             proven about it"
        ));
    }

    unsatisfied.sort();
    DeletionVerdict {
        carrier: carrier.to_string(),
        deletable: unsatisfied.is_empty(),
        unsatisfied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_more_permissive_blocks_deletion() {
        assert!(Relation::Equivalent.permits_deletion());
        assert!(Relation::Stricter.permits_deletion());
        assert!(!Relation::MorePermissive.permits_deletion());
    }

    #[test]
    fn non_equivalent_relations_require_detail() {
        assert!(!Relation::Equivalent.requires_detail());
        assert!(Relation::Stricter.requires_detail());
        assert!(Relation::MorePermissive.requires_detail());
    }

    #[test]
    fn relation_wire_spellings_are_kebab_case() {
        for (relation, wire) in [
            (Relation::Equivalent, "\"equivalent\""),
            (Relation::Stricter, "\"stricter\""),
            (Relation::MorePermissive, "\"more-permissive\""),
        ] {
            let json = serde_json::to_string(&relation).expect("relation serializes");
            assert_eq!(json, wire);
            assert_eq!(format!("\"{}\"", relation.as_str()), wire);
        }
    }

    #[test]
    fn ledger_round_trips_and_rejects_unknown_fields() {
        let raw = r#"{
          "baselineRevision": "98c26a5",
          "entries": [
            {
              "unit": "parity_corpus_tier1::node-ts",
              "carrier": "parity_corpus_tier1",
              "legacyOutcome": "pass",
              "replacementOutcome": "diverge",
              "relation": "stricter",
              "detail": "the replacement compares `configFilePath`, which `prune` removed"
            }
          ]
        }"#;
        let ledger: EquivalenceLedger = serde_json::from_str(raw).expect("ledger loads");
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].relation, Relation::Stricter);

        let bad = raw.replace("\"detail\"", "\"surprise\"");
        assert!(
            serde_json::from_str::<EquivalenceLedger>(&bad).is_err(),
            "unknown fields must be rejected"
        );
    }
}
