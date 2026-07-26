//! Scenario dimension (`sdim-`) and applicability rule (`rule-`) model
//! (024-deterministic-conformance-coverage, contracts/scenario-model.md).
//!
//! ## Two namespaces, deliberately separate
//!
//! | Namespace | File | Means | Evaluated against |
//! |---|---|---|---|
//! | `dim-*` (environment) | `dimensions.json` + `profiles.json` | **where** evidence can be gathered | the single active profile's assignment |
//! | `sdim-*` (scenario) | `scenario.json` | **what** a case exercises | a case's `scenarioContext` |
//!
//! These MUST NOT be merged (research Decision 1). `CertificationProfile.context`
//! assigns each declared *environment* dimension exactly one value, and
//! [`crate::validate::applies_in_profile`] treats a condition on an **unassigned**
//! dimension as UNSATISFIED — so a scenario dimension placed in `dimensions.json` would
//! silently drop every behavior constraining it out of profile, shrinking the very
//! denominator this feature exists to expose. A feature built to stop the denominator
//! hiding things must not begin by hiding things.
//!
//! [`Condition`] is reused **verbatim** for scenario applicability (it is just
//! dimension + value subset); only the *evaluator* is new, because a scenario condition
//! is matched against a candidate combination rather than against one active profile.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::model::Condition;

/// The scenario dimension that partitions the combination space. It is a **partition
/// key**, never a pair member: a pair covered under `up` does not cover that pair under
/// `down` (FR-013a).
pub const OPERATION_DIMENSION: &str = "sdim-operation";

/// `ScenarioDimension.kind` — a closed single-variant enum rather than a bare string, so
/// a typo is a load error instead of a silently unrecognized dimension. Environment
/// dimensions keep the `dim-` prefix in `dimensions.json` and are never represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioDimensionKind {
    /// The only kind a `scenario.json` record may declare.
    Scenario,
}

/// A scenario dimension and its closed value set (`sdim-`) — `registry/scenario.json`
/// (data-model.md §1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioDimension {
    /// `sdim-<slug>`; unique across **all** id namespaces (V2).
    pub id: String,
    /// Always [`ScenarioDimensionKind::Scenario`].
    pub kind: ScenarioDimensionKind,
    /// What this dimension varies, in one sentence.
    pub description: String,
    /// Closed, non-empty, unique, declaration-ordered value set.
    pub values: Vec<String>,
}

/// An applicability rule (`rule-`) — `registry/applicability.json` (data-model.md §2).
///
/// Rules are **pure exclusions**. There is no "include" form, no precedence, and no
/// ordering dependence: the invalidity predicate is a disjunction over rules, so
/// evaluation order cannot change the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicabilityRule {
    /// `rule-<slug>`.
    pub id: String,
    /// ≥2 conditions (V26), each naming a declared `sdim-` and a subset of its values.
    pub excludes: Vec<Condition>,
    /// Required, non-filler: states *why* the combination cannot exist (V26).
    pub ground: String,
}

/// A hand-selected high-risk triple (`hrt-`) — `registry/applicability.json`
/// (data-model.md §5). Never machine-derived (FR-016).
///
/// The records themselves are authored in User Story 3 (T079); the model and the
/// generation path exist from User Story 1 so that authoring one is a pure data edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HighRiskTriple {
    /// `hrt-<slug>`.
    pub id: String,
    /// An operation plus exactly three other scenario dimensions.
    pub assignment: IndexMap<String, String>,
    /// Required: why this interaction was selected, so a reviewer can judge the
    /// *selection* and not only the coverage.
    pub reason: String,
}

/// `registry/applicability.json` — exclusion rules and high-risk triples in one file.
///
/// Two record kinds share the file because they answer the same question ("which
/// combinations matter?") from opposite directions: `records` removes combinations that
/// cannot exist, `triples` promotes combinations whose interaction is worth pinning.
/// Mirrors the `MappingFile` precedent (`records` + `exceptions` in one document) rather
/// than forking [`crate::model::Collection`], whose `deny_unknown_fields` admits only
/// `schemaVersion` + `records`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicabilityFile {
    pub schema_version: u32,
    /// The `rule-` exclusion records.
    #[serde(default)]
    pub records: Vec<ApplicabilityRule>,
    /// The `hrt-` high-risk triples (authored in US3; empty until then).
    #[serde(default)]
    pub triples: Vec<HighRiskTriple>,
}

/// A candidate combination — total or partial — as `(dimension, value)` pairs.
///
/// A slice rather than a map because a combination has at most one entry per scenario
/// dimension (six today), so a linear scan is cheaper than hashing, and because the
/// caller's order is preserved for reporting. Lookup is by first match, which is total
/// for a well-formed combination.
pub type Combination<'a> = [(&'a str, &'a str)];

/// The value assigned to `dimension`, or `None` when the combination does not assign it.
fn value_of<'a>(combination: &Combination<'a>, dimension: &str) -> Option<&'a str> {
    combination
        .iter()
        .find(|(d, _)| *d == dimension)
        .map(|(_, v)| *v)
}

/// Whether `rule` is satisfied by `combination` — every one of its conditions names a
/// dimension the combination assigns, to a value the condition lists.
///
/// A rule constrains **only** the dimensions it names; unnamed dimensions are
/// unconstrained. A condition on a dimension the combination does not assign makes the
/// rule **inconclusive** for that partial combination, not satisfied — so a partial
/// combination is excluded only when the rule definitely forbids it, never speculatively.
///
/// A rule with no conditions is treated as satisfying nothing. Structurally such a rule
/// is a V26 violation (≥2 conditions are required); treating it as excluding *everything*
/// here would turn one malformed record into an empty denominator, which is the failure
/// mode most likely to look like success.
fn rule_excludes(rule: &ApplicabilityRule, combination: &Combination<'_>) -> bool {
    if rule.excludes.is_empty() {
        return false;
    }
    rule.excludes.iter().all(|condition| {
        value_of(combination, &condition.dimension)
            .is_some_and(|value| condition.values.iter().any(|v| v == value))
    })
}

/// The first rule (declaration order) that excludes `combination`, or `None` when the
/// combination is valid.
///
/// ```text
/// invalid(combination) ⇔ ∃ rule ∈ rules : ∀ condition ∈ rule.excludes :
///                          combination[condition.dimension] ∈ condition.values
/// ```
///
/// Exclusion is a disjunction, so *which* rule is reported is a presentation choice, not
/// a semantic one; declaration order makes it deterministic. The rule id travels with the
/// exclusion into the report (FR-012) — exclusion is attributable, silence is not.
pub fn excluding_rule<'r>(
    rules: &'r [ApplicabilityRule],
    combination: &Combination<'_>,
) -> Option<&'r ApplicabilityRule> {
    rules.iter().find(|rule| rule_excludes(rule, combination))
}

/// Whether `combination` is invalid — excluded by at least one rule.
pub fn is_invalid(rules: &[ApplicabilityRule], combination: &Combination<'_>) -> bool {
    excluding_rule(rules, combination).is_some()
}

/// The scenario model: the declared dimensions plus the exclusion rules that constrain
/// them. Borrowed from a loaded registry; pure and total, no IO.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioModel<'a> {
    /// Declaration order, as loaded.
    pub dimensions: &'a [ScenarioDimension],
    /// Declaration order, as loaded.
    pub rules: &'a [ApplicabilityRule],
}

impl<'a> ScenarioModel<'a> {
    /// Build a model over borrowed registry records.
    pub fn new(
        dimensions: &'a [ScenarioDimension],
        rules: &'a [ApplicabilityRule],
    ) -> ScenarioModel<'a> {
        ScenarioModel { dimensions, rules }
    }

    /// The dimension with `id`, or `None`.
    pub fn dimension(&self, id: &str) -> Option<&'a ScenarioDimension> {
        self.dimensions.iter().find(|d| d.id == id)
    }

    /// The partition dimension [`OPERATION_DIMENSION`], or `None` when the model does not
    /// declare it (a degenerate model; V26 reports it, generation yields no combination
    /// obligations rather than guessing a partition key).
    pub fn operation_dimension(&self) -> Option<&'a ScenarioDimension> {
        self.dimension(OPERATION_DIMENSION)
    }

    /// Every dimension except the operation partition key, in declaration order — the
    /// dimensions that may become pair members.
    pub fn pairable_dimensions(&self) -> impl Iterator<Item = &'a ScenarioDimension> {
        self.dimensions
            .iter()
            .filter(|d| d.id != OPERATION_DIMENSION)
    }

    /// The values of `dimension` that are permitted under `operation` — those for which
    /// `{operation, dimension=value}` is not excluded by any rule, in declaration order.
    pub fn permitted_values(
        &self,
        operation: &str,
        dimension: &'a ScenarioDimension,
    ) -> Vec<&'a str> {
        dimension
            .values
            .iter()
            .filter(|value| {
                !is_invalid(
                    self.rules,
                    &[
                        (OPERATION_DIMENSION, operation),
                        (dimension.id.as_str(), value.as_str()),
                    ],
                )
            })
            .map(|v| v.as_str())
            .collect()
    }

    /// The dimensions applicable under `operation`: every pairable dimension with at
    /// least one permitted value (T033, contracts/scenario-model.md "Applicability of a
    /// dimension to an operation").
    ///
    /// A dimension **all** of whose values are excluded with `operation` is inapplicable
    /// and contributes no pairs. Pruning happens **before** enumeration, not after —
    /// that is what keeps the obligation set tractable without a covering-array
    /// minimizer.
    ///
    /// Returned as `(dimension, permitted values)` so the caller never recomputes the
    /// per-value filter it just paid for.
    pub fn applicable_dimensions(
        &self,
        operation: &str,
    ) -> Vec<(&'a ScenarioDimension, Vec<&'a str>)> {
        self.pairable_dimensions()
            .filter_map(|dimension| {
                let permitted = self.permitted_values(operation, dimension);
                (!permitted.is_empty()).then_some((dimension, permitted))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim(id: &str, values: &[&str]) -> ScenarioDimension {
        ScenarioDimension {
            id: id.to_string(),
            kind: ScenarioDimensionKind::Scenario,
            description: format!("test dimension {id}"),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn condition(dimension: &str, values: &[&str]) -> Condition {
        Condition {
            dimension: dimension.to_string(),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn rule(id: &str, excludes: Vec<Condition>) -> ApplicabilityRule {
        ApplicabilityRule {
            id: id.to_string(),
            excludes,
            ground: format!("ground for {id}"),
        }
    }

    fn model_fixture() -> (Vec<ScenarioDimension>, Vec<ApplicabilityRule>) {
        let dimensions = vec![
            dim(OPERATION_DIMENSION, &["read-configuration", "up", "down"]),
            dim("sdim-container-state", &["none", "running"]),
            dim("sdim-features", &["none", "single"]),
        ];
        let rules = vec![
            rule(
                "rule-no-container-state-without-container",
                vec![
                    condition(OPERATION_DIMENSION, &["read-configuration"]),
                    condition("sdim-container-state", &["running"]),
                ],
            ),
            rule(
                "rule-no-features-for-teardown",
                vec![
                    condition(OPERATION_DIMENSION, &["down"]),
                    condition("sdim-features", &["none", "single"]),
                ],
            ),
        ];
        (dimensions, rules)
    }

    #[test]
    fn a_rule_excludes_only_when_every_condition_is_satisfied() {
        let (_dims, rules) = model_fixture();
        assert!(is_invalid(
            &rules,
            &[
                (OPERATION_DIMENSION, "read-configuration"),
                ("sdim-container-state", "running"),
            ]
        ));
        // Same operation, a value the rule does not list.
        assert!(!is_invalid(
            &rules,
            &[
                (OPERATION_DIMENSION, "read-configuration"),
                ("sdim-container-state", "none"),
            ]
        ));
        // Same state, an operation the rule does not list.
        assert!(!is_invalid(
            &rules,
            &[
                (OPERATION_DIMENSION, "up"),
                ("sdim-container-state", "running"),
            ]
        ));
    }

    #[test]
    fn a_rule_constrains_only_the_dimensions_it_names() {
        let (_dims, rules) = model_fixture();
        // A third, unnamed dimension is unconstrained: adding it changes nothing.
        assert!(is_invalid(
            &rules,
            &[
                (OPERATION_DIMENSION, "read-configuration"),
                ("sdim-container-state", "running"),
                ("sdim-features", "single"),
            ]
        ));
    }

    /// A partial combination that does not assign a dimension the rule names is
    /// INCONCLUSIVE, never excluded — otherwise a pair enumeration would drop
    /// combinations no rule actually forbids.
    #[test]
    fn a_partial_combination_missing_a_named_dimension_is_not_excluded() {
        let (_dims, rules) = model_fixture();
        assert!(
            !is_invalid(&rules, &[("sdim-container-state", "running")]),
            "without the operation the rule cannot be evaluated, so it must not exclude"
        );
    }

    #[test]
    fn exclusion_is_order_independent_and_attributable() {
        let (_dims, mut rules) = model_fixture();
        let combination = [
            (OPERATION_DIMENSION, "read-configuration"),
            ("sdim-container-state", "running"),
        ];
        let first = excluding_rule(&rules, &combination).map(|r| r.id.clone());
        assert_eq!(
            first.as_deref(),
            Some("rule-no-container-state-without-container"),
            "the excluding rule id must travel with the exclusion (FR-012)"
        );
        rules.reverse();
        assert!(
            is_invalid(&rules, &combination),
            "the predicate is a disjunction; rule order cannot change the answer"
        );
    }

    #[test]
    fn an_empty_rule_excludes_nothing() {
        let rules = vec![rule("rule-degenerate", vec![])];
        assert!(
            !is_invalid(&rules, &[(OPERATION_DIMENSION, "up")]),
            "a conditionless rule must not empty the denominator; V26 reports its shape"
        );
    }

    #[test]
    fn a_dimension_with_every_value_excluded_is_inapplicable() {
        let (dims, rules) = model_fixture();
        let model = ScenarioModel::new(&dims, &rules);

        // `down` excludes BOTH values of sdim-features, so the dimension is pruned.
        let down: Vec<&str> = model
            .applicable_dimensions("down")
            .into_iter()
            .map(|(d, _)| d.id.as_str())
            .collect();
        assert_eq!(
            down,
            vec!["sdim-container-state"],
            "a dimension all of whose values are excluded contributes no pairs"
        );

        // `read-configuration` keeps sdim-container-state with ONE surviving value.
        let read: Vec<(&str, Vec<&str>)> = model
            .applicable_dimensions("read-configuration")
            .into_iter()
            .map(|(d, v)| (d.id.as_str(), v))
            .collect();
        assert_eq!(
            read,
            vec![
                ("sdim-container-state", vec!["none"]),
                ("sdim-features", vec!["none", "single"]),
            ]
        );

        // `up` is unconstrained.
        let up: Vec<(&str, Vec<&str>)> = model
            .applicable_dimensions("up")
            .into_iter()
            .map(|(d, v)| (d.id.as_str(), v))
            .collect();
        assert_eq!(
            up,
            vec![
                ("sdim-container-state", vec!["none", "running"]),
                ("sdim-features", vec!["none", "single"]),
            ]
        );
    }

    #[test]
    fn the_operation_dimension_is_a_partition_key_not_a_pair_member() {
        let (dims, rules) = model_fixture();
        let model = ScenarioModel::new(&dims, &rules);
        assert_eq!(
            model.operation_dimension().map(|d| d.id.as_str()),
            Some(OPERATION_DIMENSION)
        );
        let pairable: Vec<&str> = model.pairable_dimensions().map(|d| d.id.as_str()).collect();
        assert_eq!(pairable, vec!["sdim-container-state", "sdim-features"]);
        for operation in ["read-configuration", "up", "down"] {
            assert!(
                model
                    .applicable_dimensions(operation)
                    .iter()
                    .all(|(d, _)| d.id != OPERATION_DIMENSION),
                "the operation dimension must never appear as a pair member (FR-013a)"
            );
        }
    }
}
