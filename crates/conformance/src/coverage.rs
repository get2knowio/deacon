//! Derived coverage evaluation (T016; FR-003, FR-017, FR-023).
//!
//! Coverage is NOT stored in the registry — it is computed from the loaded
//! [`Registry`] against the single active [`CertificationProfile`] (data-model.md
//! "Derived evaluations"). This module is the one place that classifies every
//! in-profile behavior into its coverage state and partitions the behavior
//! inventory into three disjoint groups:
//!
//! - **in-profile** behaviors (`behaviors`), each carrying its [`CoverageState`],
//!   the source units that reference it, the cases/waivers/gaps that cover it, and
//!   whether it is an extension;
//! - **out-of-profile** behaviors (`out_of_profile`), excluded from every
//!   denominator (FR-017) and reported separately;
//!
//! The denominator (FR-003) is the count of DISTINCT in-profile behaviors — never
//! source units — so a behavior referenced by several source units still counts
//! once (SC-006).
//!
//! Coverage is only meaningful for a registry that already validated (the `report`
//! / `certify` commands run validation first); the classifier therefore assumes the
//! disposition R-rules (V8) and structural coverage (V5) already hold and does not
//! re-check them. Pure in-memory computation, no IO — identical on every platform.

use std::collections::{BTreeMap, BTreeSet};

use crate::load::Registry;
use crate::model::{
    BehaviorUnit, CaseKind, CertificationProfile, Decision, ReferenceStatus, SpecStatus, TestCase,
};
use crate::obligation::{Obligation, ObligationInventory, ObligationKind};
use crate::scenario::OPERATION_DIMENSION;
use crate::validate::applies_in_profile;

/// The coverage state of a single in-profile, non-extension behavior
/// (data-model.md "Derived evaluations", FR-023). Extension behaviors are reported
/// in their own bucket and never carry one of these states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageState {
    /// Spec `conformant`, reference `aligned`, and backed by ≥1 test case.
    Conformant,
    /// Reference `divergent`, case-backed — an intentional/known divergence that
    /// the harness verifies (never folded into `Conformant`, FR-023).
    Divergent,
    /// Coverage comes from a waiver (a harness-verified characterized divergence),
    /// with no case establishing a conformant/divergent verdict. Distinct from
    /// `Conformant` (FR-023).
    Waived,
    /// Coverage comes from a gap record — a known, certification-blocking gap
    /// (FR-020, FR-025). Never reported as `Conformant`.
    Gap,
}

impl CoverageState {
    /// The wire spelling used in `report.json`'s `coverage` field.
    pub fn as_str(self) -> &'static str {
        match self {
            CoverageState::Conformant => "conformant",
            CoverageState::Divergent => "divergent",
            CoverageState::Waived => "waived",
            CoverageState::Gap => "gap",
        }
    }
}

/// The derived coverage of one in-profile behavior, with its full traceability
/// (source → behavior → case → outcome) navigable from the borrowed references.
#[derive(Debug, Clone)]
pub struct BehaviorCoverage<'a> {
    /// The behavior itself (its `applicability`, disposition axes, statement).
    pub behavior: &'a BehaviorUnit,
    /// Coverage state. Meaningful for non-extension behaviors; for extensions it is
    /// still computed but the report routes them to the extensions bucket instead.
    pub state: CoverageState,
    /// Whether this behavior is a Deacon extension (`decision: deacon-extension`) —
    /// counted in the `extensions` summary bucket, not the four coverage states.
    pub is_extension: bool,
    /// Whether the behavior has any structural coverage (case, waiver, or gap). In a
    /// valid registry this is always `true` (V5); `certify` uses `false` to surface
    /// an uncovered in-profile behavior defensively.
    pub covered: bool,
    /// IDs of the source units that reference this behavior, ID-sorted (SC-006: a
    /// behavior may have several sources but counts once in the denominator).
    pub sources: Vec<&'a str>,
    /// Test cases covering this behavior, ID-sorted — the case → outcome trace tail.
    pub cases: Vec<&'a TestCase>,
    /// IDs of waivers covering this behavior, ID-sorted.
    pub waivers: Vec<&'a str>,
    /// IDs of gaps covering this behavior, ID-sorted.
    pub gaps: Vec<&'a str>,
}

/// The complete derived coverage of a registry against its active profile.
#[derive(Debug, Clone)]
pub struct Coverage<'a> {
    /// The active profile (exactly one is active). `None` only for a degenerate
    /// registry with no active profile, in which case nothing is in-profile.
    pub profile: Option<&'a CertificationProfile>,
    /// Every in-profile behavior (extensions included, flagged), ID-sorted. This is
    /// the deduplicated denominator population (FR-003).
    pub behaviors: Vec<BehaviorCoverage<'a>>,
    /// Behaviors outside the active profile's context, ID-sorted — excluded from
    /// every denominator (FR-017), reported separately.
    pub out_of_profile: Vec<&'a BehaviorUnit>,
}

impl<'a> Coverage<'a> {
    /// Evaluate coverage of `registry` against its single active profile.
    pub fn evaluate(registry: &'a Registry) -> Coverage<'a> {
        let profile = registry.profiles.iter().find(|p| p.active);

        // Inverse indices: behavior id → the records that reference it. `BTreeMap`
        // keeps keys sorted; each value is sorted before use for determinism.
        let sources_of = invert(registry.sources.iter().map(|s| (&s.id, &s.behaviors)));
        let cases_by_behavior = case_index(registry);
        let waivers_of = invert(registry.waivers.iter().map(|w| (&w.id, &w.behaviors)));
        let gaps_of = invert(registry.gaps.iter().map(|g| (&g.id, &g.behaviors)));

        let mut in_profile: Vec<BehaviorCoverage<'a>> = Vec::new();
        let mut out_of_profile: Vec<&'a BehaviorUnit> = Vec::new();

        for bhv in &registry.behaviors {
            let id = bhv.id.as_str();

            // Out-of-profile behaviors are excluded from denominators (FR-017).
            let inside = profile.is_some_and(|p| applies_in_profile(bhv, p));
            if !inside {
                out_of_profile.push(bhv);
                continue;
            }

            let sources = sorted_refs(sources_of.get(id));
            let mut cases: Vec<&'a TestCase> =
                cases_by_behavior.get(id).cloned().unwrap_or_default();
            cases.sort_by(|a, b| a.id.cmp(&b.id));
            let waivers = sorted_refs(waivers_of.get(id));
            let gaps = sorted_refs(gaps_of.get(id));

            let has_case = !cases.is_empty();
            let has_waiver = !waivers.is_empty();
            let has_gap = !gaps.is_empty();
            let is_extension = bhv.decision == Decision::DeaconExtension;

            in_profile.push(BehaviorCoverage {
                behavior: bhv,
                state: classify(bhv, has_case, has_waiver, has_gap),
                is_extension,
                covered: has_case || has_waiver || has_gap,
                sources,
                cases,
                waivers,
                gaps,
            });
        }

        in_profile.sort_by(|a, b| a.behavior.id.cmp(&b.behavior.id));
        out_of_profile.sort_by(|a, b| a.id.cmp(&b.id));

        Coverage {
            profile,
            behaviors: in_profile,
            out_of_profile,
        }
    }

    /// The FR-003 denominator: distinct in-profile behaviors (extensions included).
    pub fn behaviors_in_profile(&self) -> usize {
        self.behaviors.len()
    }

    /// Count of in-profile, non-extension behaviors in coverage state `state`.
    pub fn count_state(&self, state: CoverageState) -> usize {
        self.behaviors
            .iter()
            .filter(|b| !b.is_extension && b.state == state)
            .count()
    }

    /// Count of in-profile behaviors classified as Deacon extensions.
    pub fn extensions_count(&self) -> usize {
        self.behaviors.iter().filter(|b| b.is_extension).count()
    }

    /// Count of out-of-profile behaviors (FR-017 — excluded from the denominator).
    pub fn out_of_profile_count(&self) -> usize {
        self.out_of_profile.len()
    }

    /// In-profile behaviors with no structural coverage (no case, waiver, or gap) —
    /// the `certify` "uncovered" blockers. Empty for a valid registry (V5).
    pub fn uncovered(&self) -> Vec<&'a BehaviorUnit> {
        self.behaviors
            .iter()
            .filter(|b| !b.covered)
            .map(|b| b.behavior)
            .collect()
    }
}

/// Classify a non-extension in-profile behavior's coverage state per the
/// data-model.md definitions.
///
/// Precedence (a valid registry makes these effectively disjoint; the ordering only
/// disambiguates the rare multi-coverage record):
/// 1. case-backed, spec `conformant` ∧ reference `aligned` → `Conformant`;
/// 2. case-backed, reference `divergent` → `Divergent` (never `Conformant`, FR-023);
/// 3. waiver coverage → `Waived`;
/// 4. gap coverage → `Gap`;
/// 5. any other case-backed shape → `Conformant` (a case verifies the behavior);
/// 6. no coverage → `Gap` (unreachable in a valid registry — V5 guarantees coverage;
///    classified defensively so the function is total).
fn classify(bhv: &BehaviorUnit, has_case: bool, has_waiver: bool, has_gap: bool) -> CoverageState {
    if has_case && bhv.spec == SpecStatus::Conformant && bhv.reference == ReferenceStatus::Aligned {
        CoverageState::Conformant
    } else if has_case && bhv.reference == ReferenceStatus::Divergent {
        CoverageState::Divergent
    } else if has_waiver {
        CoverageState::Waived
    } else if has_gap {
        CoverageState::Gap
    } else if has_case {
        CoverageState::Conformant
    } else {
        CoverageState::Gap
    }
}

/// Build behavior-id → owning-record-ids from an iterator of `(record id, its
/// behavior links)`, so each behavior maps to every record that references it.
fn invert<'a, I>(records: I) -> BTreeMap<&'a str, Vec<&'a str>>
where
    I: Iterator<Item = (&'a String, &'a Vec<String>)>,
{
    let mut map: BTreeMap<&'a str, Vec<&'a str>> = BTreeMap::new();
    for (record_id, behaviors) in records {
        for bhv in behaviors {
            map.entry(bhv.as_str())
                .or_default()
                .push(record_id.as_str());
        }
    }
    map
}

/// Build behavior-id → the test cases covering it (borrowed, for the outcome trace).
fn case_index(registry: &Registry) -> BTreeMap<&str, Vec<&TestCase>> {
    let mut map: BTreeMap<&str, Vec<&TestCase>> = BTreeMap::new();
    for case in &registry.cases {
        for bhv in &case.behaviors {
            map.entry(bhv.as_str()).or_default().push(case);
        }
    }
    map
}

/// Clone-and-sort the owning-record IDs for one behavior (or empty when absent).
fn sorted_refs<'a>(refs: Option<&Vec<&'a str>>) -> Vec<&'a str> {
    let mut out = refs.cloned().unwrap_or_default();
    out.sort_unstable();
    out
}

// ---------------------------------------------------------------------------
// Obligation coverage (024-deterministic-conformance-coverage, T043)
// ---------------------------------------------------------------------------

/// The reporting bucket an obligation falls into.
///
/// The five FR-026 buckets are **never folded together**; `Undispositioned` is the sixth,
/// honest state for an obligation that carries no explicit disposition record yet — the
/// number SC-001 requires to reach zero. Folding it into `Gap` would overstate what is
/// known; folding it into `Covered` would understate the hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObligationBucket {
    /// Satisfied by ≥1 executable case.
    Covered,
    /// Satisfied by a waiver (a harness-verified characterized divergence).
    Waived,
    /// Explicitly declared untestable, with a ground.
    NonTestable,
    /// An unresolved gap — always blocking.
    Gap,
    /// Modelled but not exercisable in the active environment profile: enumerated as a
    /// visible backlog, counted as neither covered nor gap (FR-004a, spec Assumption 11).
    InactiveEnvironment,
    /// No explicit disposition. Not a state anyone chose.
    Undispositioned,
}

impl ObligationBucket {
    /// The wire spelling used in the coverage reports.
    pub fn as_str(self) -> &'static str {
        match self {
            ObligationBucket::Covered => "covered",
            ObligationBucket::Waived => "waived",
            ObligationBucket::NonTestable => "non-testable",
            ObligationBucket::Gap => "gap",
            ObligationBucket::InactiveEnvironment => "inactive-environment",
            ObligationBucket::Undispositioned => "undispositioned",
        }
    }
}

/// One obligation's evaluated bucket plus the ids that back it.
#[derive(Debug, Clone)]
pub struct ObligationOutcome<'a> {
    pub obligation: &'a Obligation,
    pub bucket: ObligationBucket,
    /// The covering case ids, or the backing waiver/gap ids — id-sorted. Empty for
    /// `Undispositioned` and `InactiveEnvironment`.
    pub by: Vec<String>,
}

/// Evaluate every obligation in `inventory` against `registry`, in inventory (id) order.
///
/// **Combination obligations** are matched per contracts/scenario-model.md "Coverage
/// matching": a pair `(o, d₁=v₁, d₂=v₂)` is covered iff some case satisfies all three of
///
/// 1. `scenarioContext[sdim-operation] == o`,
/// 2. `scenarioContext[d₁] == v₁` and `scenarioContext[d₂] == v₂`,
/// 3. the case is **executable** — not a legacy carrier, except while an open residual
///    names its carrier (the Edge Case ruling; see [`executable_case_ids`]).
///
/// Because a case assigns *every* dimension, one case covers `C(n,2)` pairs at once under
/// its operation. That density is why the pair space is fillable at all, and why the
/// report's job is to name the sparse remainder.
///
/// **Behavior obligations** reuse the established per-behavior coverage
/// ([`Coverage::evaluate`]) rather than a second, forkable evaluator: an out-of-profile
/// behavior is `inactive-environment`; otherwise its [`CoverageState`] maps onto the
/// bucket vocabulary. Reusing it means the behavior denominator `certify` already
/// measures and the obligation denominator this feature adds can never disagree.
///
/// Explicit `odp-` disposition records (User Story 2) take precedence over everything
/// here; until they exist, this is evidence-derived and an obligation with no evidence is
/// [`ObligationBucket::Undispositioned`].
pub fn evaluate_obligations<'a>(
    registry: &'a Registry,
    inventory: &'a ObligationInventory,
) -> Vec<ObligationOutcome<'a>> {
    let coverage = Coverage::evaluate(registry);
    let by_behavior: BTreeMap<&str, &BehaviorCoverage<'a>> = coverage
        .behaviors
        .iter()
        .map(|b| (b.behavior.id.as_str(), b))
        .collect();
    let out_of_profile: BTreeSet<&str> = coverage
        .out_of_profile
        .iter()
        .map(|b| b.id.as_str())
        .collect();

    let executable = executable_case_ids(registry);
    let scenario_cases: Vec<&TestCase> = registry
        .cases
        .iter()
        .filter(|c| !c.scenario_context.is_empty() && executable.contains(c.id.as_str()))
        .collect();

    inventory
        .units
        .iter()
        .map(|unit| match unit.kind {
            ObligationKind::Combination => {
                let by = matching_case_ids(unit, &scenario_cases);
                ObligationOutcome {
                    obligation: unit,
                    bucket: if by.is_empty() {
                        ObligationBucket::Undispositioned
                    } else {
                        ObligationBucket::Covered
                    },
                    by,
                }
            }
            ObligationKind::Behavior => {
                let behavior = unit.behavior.as_deref().unwrap_or_default();
                if out_of_profile.contains(behavior) {
                    return ObligationOutcome {
                        obligation: unit,
                        bucket: ObligationBucket::InactiveEnvironment,
                        by: Vec::new(),
                    };
                }
                match by_behavior.get(behavior) {
                    None => ObligationOutcome {
                        obligation: unit,
                        bucket: ObligationBucket::Undispositioned,
                        by: Vec::new(),
                    },
                    Some(entry) => {
                        let (bucket, by): (ObligationBucket, Vec<String>) = match entry.state {
                            CoverageState::Conformant | CoverageState::Divergent => (
                                ObligationBucket::Covered,
                                entry.cases.iter().map(|c| c.id.clone()).collect(),
                            ),
                            CoverageState::Waived => (
                                ObligationBucket::Waived,
                                entry.waivers.iter().map(|w| (*w).to_string()).collect(),
                            ),
                            CoverageState::Gap => (
                                ObligationBucket::Gap,
                                entry.gaps.iter().map(|g| (*g).to_string()).collect(),
                            ),
                        };
                        if by.is_empty() {
                            ObligationOutcome {
                                obligation: unit,
                                bucket: ObligationBucket::Undispositioned,
                                by,
                            }
                        } else {
                            ObligationOutcome {
                                obligation: unit,
                                bucket,
                                by,
                            }
                        }
                    }
                }
            }
        })
        .collect()
}

/// The ids of cases whose evidence may satisfy a combination obligation.
///
/// A declarative case is executable by construction. A **legacy** case is executable only
/// while an open residual keeps its carrier alive — the residual is the record that says
/// "the coverage still exists, carried by a program not yet retired". Once the residual is
/// gone and the carrier deleted, the legacy record stops satisfying anything, which is
/// exactly the decay the residual model exists to produce.
pub fn executable_case_ids(registry: &Registry) -> BTreeSet<&str> {
    let live_carriers: BTreeSet<&str> = registry
        .residuals
        .iter()
        .filter_map(|r| r.blocked_carrier.as_deref())
        .collect();
    registry
        .cases
        .iter()
        .filter(|case| match case.classify() {
            Ok(CaseKind::Declarative) => true,
            Ok(CaseKind::Legacy) => case
                .executable
                .as_ref()
                .is_some_and(|e| live_carriers.contains(e.binary.as_str())),
            // A malformed record is rejected at load; treat it as non-evidence here so a
            // shape error can never silently satisfy an obligation.
            Err(_) => false,
        })
        .map(|case| case.id.as_str())
        .collect()
}

/// The ids of cases whose scenario context matches a combination obligation, id-sorted.
fn matching_case_ids(unit: &Obligation, cases: &[&TestCase]) -> Vec<String> {
    let (Some(operation), Some(assignment)) = (unit.operation.as_deref(), unit.assignment.as_ref())
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = cases
        .iter()
        .filter(|case| {
            case.scenario_context
                .get(OPERATION_DIMENSION)
                .is_some_and(|o| o == operation)
                && assignment.iter().all(|(dimension, value)| {
                    case.scenario_context
                        .get(dimension)
                        .is_some_and(|assigned| assigned == value)
                })
        })
        .map(|case| case.id.clone())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Condition, Executable, Expect, ExpectedOutcome, Scope, SourceUnit, Waiver};
    use indexmap::IndexMap;

    fn behavior(
        id: &str,
        spec: SpecStatus,
        reference: ReferenceStatus,
        decision: Decision,
        applicability: Vec<Condition>,
    ) -> BehaviorUnit {
        BehaviorUnit {
            id: id.to_string(),
            area: "test".to_string(),
            statement: format!("statement for {id}"),
            applicability,
            spec,
            reference,
            decision,
            notes: None,
        }
    }

    fn cond(dimension: &str, values: &[&str]) -> Condition {
        Condition {
            dimension: dimension.to_string(),
            values: values.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn docker_profile() -> CertificationProfile {
        let mut context = IndexMap::new();
        context.insert("dim-runtime".to_string(), "docker".to_string());
        CertificationProfile {
            id: "prof-x".to_string(),
            context,
            active: true,
        }
    }

    fn source(id: &str, behaviors: &[&str]) -> SourceUnit {
        SourceUnit {
            id: id.to_string(),
            inventory: crate::model::Inventory::Observed,
            revision: "rev-oracle-0-87-0".to_string(),
            locator: "loc".to_string(),
            summary: "s".to_string(),
            behaviors: behaviors.iter().map(|b| b.to_string()).collect(),
            out_of_scope: None,
        }
    }

    fn case(id: &str, behaviors: &[&str]) -> TestCase {
        TestCase {
            id: id.to_string(),
            behaviors: behaviors.iter().map(|b| b.to_string()).collect(),
            context: vec![],
            executable: Some(Executable {
                binary: "some_binary".to_string(),
                test: None,
                corpus: None,
                case: None,
            }),
            outcomes: vec![ExpectedOutcome {
                channel: "chan-exit-code".to_string(),
                expectation: "exits 0".to_string(),
            }],
            ..TestCase::default()
        }
    }

    fn waiver(id: &str, behaviors: &[&str]) -> Waiver {
        Waiver {
            id: id.to_string(),
            behaviors: behaviors.iter().map(|b| b.to_string()).collect(),
            scope: Scope::CorpusCase {
                corpus: "errors".to_string(),
                case: "malformed-json".to_string(),
            },
            expect: Expect::DeaconStricter { signal: None },
            rationale: "r".to_string(),
            added: "2026-07-19".to_string(),
            expires: "2027-01-19".to_string(),
            config: None,
        }
    }

    #[test]
    fn classifies_each_coverage_state_and_partitions_by_profile() {
        let mut reg = Registry::default();
        reg.profiles.push(docker_profile());

        // conformant: case-backed, spec conformant, reference aligned.
        reg.behaviors.push(behavior(
            "bhv-conformant",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        ));
        // divergent: case-backed, reference divergent.
        reg.behaviors.push(behavior(
            "bhv-divergent",
            SpecStatus::Unspecified,
            ReferenceStatus::Divergent,
            Decision::IntentionalDivergence,
            vec![],
        ));
        // waived: waiver-backed only.
        reg.behaviors.push(behavior(
            "bhv-waived",
            SpecStatus::Unspecified,
            ReferenceStatus::Divergent,
            Decision::IntentionalDivergence,
            vec![],
        ));
        // gap: gap-backed only.
        reg.behaviors.push(behavior(
            "bhv-gap",
            SpecStatus::Unspecified,
            ReferenceStatus::Unknown,
            Decision::UnresolvedGap,
            vec![],
        ));
        // extension: decision deacon-extension (own bucket).
        reg.behaviors.push(behavior(
            "bhv-ext",
            SpecStatus::Unspecified,
            ReferenceStatus::NotApplicable,
            Decision::DeaconExtension,
            vec![],
        ));
        // out-of-profile: podman-only under an active docker profile.
        reg.behaviors.push(behavior(
            "bhv-out",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![cond("dim-runtime", &["podman"])],
        ));

        reg.cases.push(case("case-conformant", &["bhv-conformant"]));
        reg.cases.push(case("case-divergent", &["bhv-divergent"]));
        reg.cases.push(case("case-ext", &["bhv-ext"]));
        reg.waivers.push(waiver("wvr-waived", &["bhv-waived"]));
        reg.gaps.push(crate::model::Gap {
            id: "gap-gap".to_string(),
            kind: crate::model::GapKind::Knowledge,
            behaviors: vec!["bhv-gap".to_string()],
            description: "d".to_string(),
            tracking: None,
        });

        let coverage = Coverage::evaluate(&reg);

        // Five in-profile behaviors (the podman one is out-of-profile).
        assert_eq!(coverage.behaviors_in_profile(), 5);
        assert_eq!(coverage.out_of_profile_count(), 1);
        assert_eq!(coverage.out_of_profile[0].id, "bhv-out");

        assert_eq!(coverage.count_state(CoverageState::Conformant), 1);
        assert_eq!(coverage.count_state(CoverageState::Divergent), 1);
        assert_eq!(coverage.count_state(CoverageState::Waived), 1);
        assert_eq!(coverage.count_state(CoverageState::Gap), 1);
        assert_eq!(coverage.extensions_count(), 1);

        // The five state/extension buckets sum to the denominator.
        let sum = coverage.count_state(CoverageState::Conformant)
            + coverage.count_state(CoverageState::Divergent)
            + coverage.count_state(CoverageState::Waived)
            + coverage.count_state(CoverageState::Gap)
            + coverage.extensions_count();
        assert_eq!(sum, coverage.behaviors_in_profile());

        // No uncovered behaviors — every in-profile behavior has case/waiver/gap.
        assert!(coverage.uncovered().is_empty());
    }

    #[test]
    fn several_sources_map_to_one_behavior_but_count_once() {
        // SC-006: a behavior referenced by several source units counts ONCE in the
        // denominator, and all its sources are traced.
        let mut reg = Registry::default();
        reg.profiles.push(docker_profile());
        reg.behaviors.push(behavior(
            "bhv-multi",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        ));
        reg.sources.push(source("src-obs-c", &["bhv-multi"]));
        reg.sources.push(source("src-obs-a", &["bhv-multi"]));
        reg.sources.push(source("src-spec-b", &["bhv-multi"]));
        reg.cases.push(case("case-multi", &["bhv-multi"]));

        let coverage = Coverage::evaluate(&reg);

        assert_eq!(
            coverage.behaviors_in_profile(),
            1,
            "three sources must not inflate the behavior denominator"
        );
        let entry = &coverage.behaviors[0];
        assert_eq!(entry.behavior.id, "bhv-multi");
        // All three sources are traced, ID-sorted.
        assert_eq!(entry.sources, vec!["src-obs-a", "src-obs-c", "src-spec-b"]);
        assert_eq!(entry.state, CoverageState::Conformant);
    }

    #[test]
    fn uncovered_behavior_is_reported_by_certify_helper() {
        // A degenerate in-profile behavior with no coverage (would be V5-invalid) is
        // surfaced by `uncovered()` for defensive certification.
        let mut reg = Registry::default();
        reg.profiles.push(docker_profile());
        reg.behaviors.push(behavior(
            "bhv-bare",
            SpecStatus::Conformant,
            ReferenceStatus::Unknown,
            Decision::UnresolvedGap,
            vec![],
        ));
        let coverage = Coverage::evaluate(&reg);
        let uncovered = coverage.uncovered();
        assert_eq!(uncovered.len(), 1);
        assert_eq!(uncovered[0].id, "bhv-bare");
    }

    #[test]
    fn no_active_profile_puts_everything_out_of_profile() {
        let mut reg = Registry::default();
        reg.behaviors.push(behavior(
            "bhv-x",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        ));
        let coverage = Coverage::evaluate(&reg);
        assert!(coverage.profile.is_none());
        assert_eq!(coverage.behaviors_in_profile(), 0);
        assert_eq!(coverage.out_of_profile_count(), 1);
    }
}
