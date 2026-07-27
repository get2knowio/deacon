//! The coverage report renderers (024-deterministic-conformance-coverage,
//! contracts/coverage-report.md): `coverage-pairwise`, `coverage-triples`,
//! `coverage-operations`, and `coverage-observables`.
//!
//! **Universal properties**, no exceptions (FR-062, SC-010):
//!
//! | Property | How it is held |
//! |---|---|
//! | Byte-stable | ordered maps + id sorts only; no `HashMap` iteration reaches the output |
//! | No ambient inputs | no timestamps, no absolute paths, no hostname, no run-dependent ordering |
//! | Read-only | building a report never records, refreshes, or repairs evidence (FR-063) |
//! | Non-gating | the exit code reflects whether the report could be *written*, never what it says |
//!
//! Each `.md` is rendered from the **same in-memory model** as its `.json`, in the same
//! order — never assembled separately. A discrepancy between the two would mean the
//! human-readable artifact and the machine-readable one disagree about coverage, and the
//! human one is what gets reviewed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Serialize;

use crate::coverage::{ObligationBucket, ObligationOutcome, evaluate_obligations};
use crate::load::Registry;
use crate::model::{InputClass, TestCase};
use crate::obligation::{ObligationInventory, ObligationKind};
use crate::scenario::{OPERATION_DIMENSION, ScenarioModel, excluding_rule};

/// Schema version of every coverage report document.
pub const COVERAGE_REPORT_SCHEMA_VERSION: u32 = 1;

/// The five input classes FR-040 requires cases to span, in report order.
///
/// A case now **declares** its class ([`TestCase::input_class`], 024 US3); the derivation
/// in [`input_class_of`] remains the fallback for the records that predate the field. Two
/// of the five — `boundary` and `unsupported` — have no derivable signal at all, so before
/// the field existed they were reported permanently missing however many such cases were
/// written. That was the honest answer to a question inference could not answer; declaring
/// the class is the answer to the question itself.
pub const INPUT_CLASSES: &[&str] = &[
    "valid",
    "boundary",
    "malformed",
    "unsupported",
    "reference-lenient",
];

/// The twelve fields broad normalization used to hide (FR-047 – FR-055), each with the
/// observable-path fragments that evidence a case really compares it.
///
/// Hand-listed rather than derived: they are the *named* US5 targets, and deriving them
/// from whatever the record happens to contain would make SC-008 measure itself.
pub const DENORMALIZED_FIELDS: &[(&str, &[&str])] = &[
    (
        "lifecycle-array-vs-object",
        &[
            "lifecycle",
            "oncreate",
            "postcreate",
            "poststart",
            "postattach",
            "updatecontent",
        ],
    ),
    ("command", &["cmd", "command"]),
    ("entrypoint-chained", &["entrypoint"]),
    (
        "env-merge-precedence",
        &["env", "containerenv", "remoteenv"],
    ),
    ("path-construction", &["path"]),
    ("user-uid-gid", &["user", "uid", "gid"]),
    ("metadata-label-namespaces", &["label", "metadata"]),
    ("mount-source", &["mount.source", "mounts.source", "source"]),
    ("mount-shape", &["mount", "mounts"]),
    ("network", &["network"]),
    ("compose-project-resources", &["compose", "project"]),
    ("null-empty-omitted", &["null", "empty", "omitted"]),
];

// ---------------------------------------------------------------------------
// §1 coverage-pairwise
// ---------------------------------------------------------------------------

/// `coverage-pairwise.json` (contracts/coverage-report.md §1).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairwiseReport {
    pub schema_version: u32,
    /// One entry per declared operation, in declaration order.
    pub operations: Vec<OperationPairs>,
    pub summary: PairwiseSummary,
    /// Values appearing in no valid combination (V26, FR-010).
    pub dead_values: Vec<DeadValue>,
}

/// The combination obligations, exclusions, and applicable dimensions of one operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationPairs {
    pub operation: String,
    /// Dimensions with ≥1 permitted value under this operation, declaration-ordered. A
    /// dimension absent here was pruned before enumeration (T033).
    pub applicable_dimensions: Vec<String>,
    pub pairs: Vec<PairEntry>,
    /// Invalid combinations **with the rule that excluded them** (FR-012).
    ///
    /// This list exists so that "absent because impossible" is visibly different from
    /// "absent because nobody wrote it". Collapsing the two would make the denominator
    /// unfalsifiable.
    pub excluded: Vec<ExcludedEntry>,
}

/// One combination obligation and its bucket.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairEntry {
    pub obligation: String,
    pub assignment: IndexMap<String, String>,
    /// `2` for a pair, `3` for a high-risk triple.
    pub arity: u32,
    /// One of the five FR-026 buckets, or `undispositioned` — never folded together.
    pub bucket: String,
    /// The covering case ids, or the backing waiver/gap ids.
    pub by: Vec<String>,
}

/// An excluded combination and the rule that excluded it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedEntry {
    pub assignment: IndexMap<String, String>,
    pub rule: String,
}

/// A declared value that appears in no valid combination (V26, FR-010).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadValue {
    pub dimension: String,
    pub value: String,
}

/// The five FR-026 buckets plus `undispositioned`, counted over **every** obligation of
/// both kinds.
///
/// The population is deliberately both kinds: SC-001 requires `undispositioned` to reach
/// zero across the whole record, and a summary scoped only to combinations would let a
/// behavior obligation go unclassified without moving a single number. [`kinds`] carries
/// the split so the relationship to `operations[].pairs` stays legible.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairwiseSummary {
    /// Total applicable obligations enumerated.
    pub valid: usize,
    pub covered: usize,
    pub waived: usize,
    pub non_testable: usize,
    pub gap: usize,
    pub inactive_environment: usize,
    /// The number SC-001 requires to be zero.
    pub undispositioned: usize,
    /// `{ combination, behavior }` — how `valid` splits across the two kinds (FR-019).
    pub kinds: IndexMap<String, usize>,
}

// ---------------------------------------------------------------------------
// §2 coverage-triples
// ---------------------------------------------------------------------------

/// `coverage-triples.json` (contracts/coverage-report.md §2).
///
/// One row per hand-selected `hrt-` record, in declaration order — the order the author
/// chose, which is the order a reviewer reads the selection in. The row carries the
/// triple's `reason` verbatim so the **selection** is reviewable and not only the
/// coverage: a triple set nobody can argue with makes SC-003 a formality.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriplesReport {
    pub schema_version: u32,
    pub triples: Vec<TripleEntry>,
    pub summary: TriplesSummary,
}

/// One selected high-risk triple, its generated obligation, and how it is discharged.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TripleEntry {
    /// The `hrt-` record id.
    pub id: String,
    /// The `obl-cmb-` obligation generation derived from it, or `""` when the triple names
    /// no operation and generation therefore emitted nothing (V26 reports the record).
    pub obligation: String,
    /// The full assignment, operation first, in the record's declaration order.
    pub assignment: IndexMap<String, String>,
    /// Why this interaction was selected — carried verbatim (FR-016).
    pub reason: String,
    /// `covered` or `gap` only. FR-015 forbids rationale/waiver on a triple and V29
    /// rejects it at validation, so no other bucket can reach this report through a valid
    /// registry; an `undispositioned` triple still appears, because SC-001 counts it.
    pub bucket: String,
    /// The covering case ids, or the backing gap id.
    pub by: Vec<String>,
}

/// Triple-set summary. `selected` is the size of the hand-authored set — the number
/// SC-003 is about — and never a count of what happened to be generated.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriplesSummary {
    pub selected: usize,
    pub covered: usize,
    pub gap: usize,
    /// Triples in neither bucket — undispositioned, or (invalidly) argued rather than
    /// tested. Reported rather than folded, so a triple cannot go missing between the two
    /// buckets the contract names.
    pub other: usize,
}

// ---------------------------------------------------------------------------
// §3 coverage-operations
// ---------------------------------------------------------------------------

/// `coverage-operations.json` (contracts/coverage-report.md §3).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationsReport {
    pub schema_version: u32,
    pub operations: Vec<OperationCoverage>,
}

/// What one operation's cases exercise, and what they do not.
///
/// `missingConfigSources` is the SC-004 measure and `missingInputClasses` the FR-040 one.
/// Both list what is **absent**: a report that only counted what exists would say nothing
/// about the hole.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationCoverage {
    pub operation: String,
    /// Cases attributed to this operation (see [`case_operations`]).
    pub cases: usize,
    /// Input-class tallies, in [`INPUT_CLASSES`] order — declared where a case declares
    /// one, derived otherwise ([`input_class_of`]).
    pub input_classes: IndexMap<String, usize>,
    /// Case counts per configuration source, from declared `scenarioContext` only.
    pub config_sources: IndexMap<String, usize>,
    /// Observable channels this operation's cases compare, id-sorted.
    pub channels: Vec<String>,
    /// Whether a differential against the pinned reference can be run for this operation.
    pub differential_available: bool,
    /// When it cannot: why, and what is substituted (spec Assumption 5). Present exactly
    /// when `differentialAvailable` is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub differential_substitution: Option<String>,
    /// Permitted classes with no case. `reference-lenient` is **not** permitted for an
    /// operation the reference does not implement, so it is never reported missing there
    /// — demanding a case that cannot exist would turn an honest measure into noise.
    pub missing_input_classes: Vec<String>,
    pub missing_config_sources: Vec<String>,
}

// ---------------------------------------------------------------------------
// §4 coverage-observables
// ---------------------------------------------------------------------------

/// `coverage-observables.json` (contracts/coverage-report.md §4).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservablesReport {
    pub schema_version: u32,
    pub channels: Vec<ChannelCoverage>,
    pub denormalized_fields: Vec<DenormalizedField>,
    /// MUST be empty; a non-empty list is V24 and blocks (FR-056).
    pub unscoped_normalization_rules: Vec<String>,
    pub summary: ObservablesSummary,
}

/// One channel's comparison surface.
///
/// `fields` lists what is actually **compared**, not what is captured. The distinction is
/// the point of the report: 023 found two real defects the moment a captured-but-uncompared
/// field started being compared, and a report that counted captures would have shown those
/// channels as healthy the whole time. A path is evidence of comparison when a case
/// asserts on it (`jsonSubset`) or tolerates a difference at it (an allowed difference is
/// only ever needed because the comparison reaches that path).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelCoverage {
    pub channel: String,
    pub cases: usize,
    pub fields: Vec<String>,
    pub denormalized_fields_covered: Vec<String>,
}

/// One of the twelve US5 fields and the cases that compare it (the SC-008 measure).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DenormalizedField {
    pub field: String,
    pub covered: bool,
    pub by: Vec<String>,
}

/// Channel-floor summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservablesSummary {
    /// Channels compared by fewer than [`CHANNEL_CASE_FLOOR`] cases — the SC-005 measure.
    pub channels_below_floor: usize,
}

/// The minimum number of cases that must compare a channel before it counts as exercised
/// (SC-005).
pub const CHANNEL_CASE_FLOOR: usize = 3;

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// The three report families, built once from one in-memory model so the `.json` and the
/// `.md` can never disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageReports {
    pub pairwise: PairwiseReport,
    pub triples: TriplesReport,
    pub operations: OperationsReport,
    pub observables: ObservablesReport,
}

/// Build every coverage report from a loaded registry and its generated obligations.
///
/// Pure and total: no IO, no clock, no environment. The same registry content produces the
/// same reports on every machine.
pub fn build_coverage_reports(
    registry: &Registry,
    inventory: &ObligationInventory,
) -> CoverageReports {
    let outcomes = evaluate_obligations(registry, inventory);
    CoverageReports {
        pairwise: build_pairwise(registry, &outcomes),
        triples: build_triples(registry, &outcomes),
        operations: build_operations(registry),
        observables: build_observables(registry),
    }
}

/// §2: one row per hand-selected `hrt-` record, joined to the obligation generation
/// derived from it.
///
/// The join key is the **obligation id**, recomputed from the triple exactly as
/// `generate_triples` computes it, rather than a positional or arity-based match against
/// the inventory. Identity is substance-anchored (contracts/obligation.md), so recomputing
/// is the only join that stays correct when a triple is reordered, renamed, or when a
/// *pair* obligation happens to share the assignment — the id is what the disposition
/// names, so the id is what the report must follow.
fn build_triples(registry: &Registry, outcomes: &[ObligationOutcome<'_>]) -> TriplesReport {
    let by_id: BTreeMap<&str, &ObligationOutcome<'_>> = outcomes
        .iter()
        .map(|outcome| (outcome.obligation.id.as_str(), outcome))
        .collect();

    let mut triples = Vec::new();
    let (mut covered, mut gap, mut other) = (0usize, 0usize, 0usize);
    for triple in &registry.triples {
        let obligation = crate::obligation::triple_obligation_id(triple);
        let outcome = obligation.as_deref().and_then(|id| by_id.get(id));
        let (bucket, by) = match outcome {
            Some(outcome) => (outcome.bucket.as_str().to_string(), outcome.by.clone()),
            // A triple that generated no obligation cannot be dispositioned at all. Saying
            // `gap` would be a coverage claim about a combination the model never emitted;
            // V26 names the modelling mistake, and this row records that it has no bucket.
            None => (
                ObligationBucket::Undispositioned.as_str().to_string(),
                Vec::new(),
            ),
        };
        match bucket.as_str() {
            b if b == ObligationBucket::Covered.as_str() => covered += 1,
            b if b == ObligationBucket::Gap.as_str() => gap += 1,
            _ => other += 1,
        }
        triples.push(TripleEntry {
            id: triple.id.clone(),
            obligation: obligation.unwrap_or_default(),
            assignment: triple.assignment.clone(),
            reason: triple.reason.clone(),
            bucket,
            by,
        });
    }

    TriplesReport {
        schema_version: COVERAGE_REPORT_SCHEMA_VERSION,
        summary: TriplesSummary {
            selected: triples.len(),
            covered,
            gap,
            other,
        },
        triples,
    }
}

fn build_pairwise(registry: &Registry, outcomes: &[ObligationOutcome<'_>]) -> PairwiseReport {
    let model = ScenarioModel::new(&registry.scenario, &registry.applicability);

    // Combination outcomes grouped by their operation, keeping inventory (id) order.
    let mut by_operation: BTreeMap<&str, Vec<&ObligationOutcome<'_>>> = BTreeMap::new();
    for outcome in outcomes {
        if outcome.obligation.kind != ObligationKind::Combination {
            continue;
        }
        if let Some(operation) = outcome.obligation.operation.as_deref() {
            by_operation.entry(operation).or_default().push(outcome);
        }
    }

    let mut operations = Vec::new();
    let mut alive: BTreeSet<(&str, &str)> = BTreeSet::new();
    if let Some(operation_dimension) = model.operation_dimension() {
        for operation in &operation_dimension.values {
            let applicable = model.applicable_dimensions(operation);
            let pairs: Vec<PairEntry> = by_operation
                .get(operation.as_str())
                .map(|entries| {
                    entries
                        .iter()
                        .map(|outcome| {
                            let assignment =
                                outcome.obligation.assignment.clone().unwrap_or_default();
                            // Liveness is computed from the PAIR enumeration only, matching
                            // `validate::check_scenario_model`'s definition of a dead value.
                            // A hand-selected triple naming a value the pair space cannot
                            // reach is itself a V26 violation ("selects a combination a rule
                            // excludes"), so letting it rescue the value here would give the
                            // report and the validator two different answers to the same
                            // question — and the report's answer would be the one that hid
                            // the modelling mistake.
                            if outcome.obligation.arity.unwrap_or(2) == 2 {
                                for (dimension, value) in &assignment {
                                    // Borrowed from the registry, not from the clone.
                                    if let Some(declared) = model.dimension(dimension)
                                        && let Some(v) =
                                            declared.values.iter().find(|d| *d == value)
                                    {
                                        alive.insert((declared.id.as_str(), v.as_str()));
                                    }
                                }
                            }
                            PairEntry {
                                obligation: outcome.obligation.id.clone(),
                                assignment,
                                arity: outcome.obligation.arity.unwrap_or(2),
                                bucket: outcome.bucket.as_str().to_string(),
                                by: outcome.by.clone(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !pairs.is_empty()
                && let Some(declared) = model.operation_dimension()
                && let Some(v) = declared.values.iter().find(|d| *d == operation)
            {
                alive.insert((declared.id.as_str(), v.as_str()));
            }

            operations.push(OperationPairs {
                operation: operation.clone(),
                applicable_dimensions: applicable.iter().map(|(d, _)| d.id.clone()).collect(),
                pairs,
                excluded: excluded_entries(&model, operation),
            });
        }
    }

    let mut dead_values = Vec::new();
    for dimension in &registry.scenario {
        for value in &dimension.values {
            if !alive.contains(&(dimension.id.as_str(), value.as_str())) {
                dead_values.push(DeadValue {
                    dimension: dimension.id.clone(),
                    value: value.clone(),
                });
            }
        }
    }

    let count = |bucket: ObligationBucket| outcomes.iter().filter(|o| o.bucket == bucket).count();
    let combinations = outcomes
        .iter()
        .filter(|o| o.obligation.kind == ObligationKind::Combination)
        .count();
    let mut kinds = IndexMap::new();
    kinds.insert("combination".to_string(), combinations);
    kinds.insert("behavior".to_string(), outcomes.len() - combinations);

    PairwiseReport {
        schema_version: COVERAGE_REPORT_SCHEMA_VERSION,
        operations,
        summary: PairwiseSummary {
            valid: outcomes.len(),
            covered: count(ObligationBucket::Covered),
            waived: count(ObligationBucket::Waived),
            non_testable: count(ObligationBucket::NonTestable),
            gap: count(ObligationBucket::Gap),
            inactive_environment: count(ObligationBucket::InactiveEnvironment),
            undispositioned: count(ObligationBucket::Undispositioned),
            kinds,
        },
        dead_values,
    }
}

/// The combinations an applicability rule removes from `operation`'s space, each carrying
/// the excluding rule id (FR-012).
///
/// Single-dimension entries come first (a whole value ruled out under this operation, the
/// shape data-model.md §1's example uses), then value pairs excluded by a rule that names
/// both pair members — the two ways a combination can disappear, both attributed.
fn excluded_entries(model: &ScenarioModel<'_>, operation: &str) -> Vec<ExcludedEntry> {
    let mut out = Vec::new();

    for dimension in model.pairable_dimensions() {
        let permitted: BTreeSet<&str> = model
            .permitted_values(operation, dimension)
            .into_iter()
            .collect();
        for value in &dimension.values {
            if permitted.contains(value.as_str()) {
                continue;
            }
            let combination = [
                (OPERATION_DIMENSION, operation),
                (dimension.id.as_str(), value.as_str()),
            ];
            if let Some(rule) = excluding_rule(model.rules, &combination) {
                let mut assignment = IndexMap::new();
                assignment.insert(dimension.id.clone(), value.clone());
                out.push(ExcludedEntry {
                    assignment,
                    rule: rule.id.clone(),
                });
            }
        }
    }

    // Pair-level exclusions: only meaningful between dimensions that both survived, and
    // only when a rule names both — otherwise the single-dimension entry above already
    // explains the absence and repeating it would inflate the list.
    let applicable = model.applicable_dimensions(operation);
    for (i, (first, first_values)) in applicable.iter().enumerate() {
        for (second, second_values) in applicable.iter().skip(i + 1) {
            for a in first_values {
                for b in second_values {
                    let combination = [
                        (OPERATION_DIMENSION, operation),
                        (first.id.as_str(), *a),
                        (second.id.as_str(), *b),
                    ];
                    let Some(rule) = excluding_rule(model.rules, &combination) else {
                        continue;
                    };
                    let mut assignment = IndexMap::new();
                    assignment.insert(first.id.clone(), (*a).to_string());
                    assignment.insert(second.id.clone(), (*b).to_string());
                    out.push(ExcludedEntry {
                        assignment,
                        rule: rule.id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// The operations a case exercises.
///
/// A case's `scenarioContext` is authoritative when it declares one. Otherwise the
/// operations are the distinct consumer subcommands the case actually invokes — real,
/// already-declared data, which keeps this report informative for the 88 cases that
/// predate the scenario model instead of reporting every operation as untouched. A legacy
/// (binary-backed) case invokes nothing declaratively and is therefore attributed to no
/// operation; that absence is itself part of the measured hole.
fn case_operations(case: &TestCase) -> BTreeSet<String> {
    if let Some(operation) = case.scenario_context.get(OPERATION_DIMENSION) {
        return BTreeSet::from([operation.clone()]);
    }
    case.operations
        .iter()
        .map(|op| op.subcommand.clone())
        .collect()
}

/// The input class of a case: its **declared** class when it has one, otherwise the
/// pre-US3 derivation ([`INPUT_CLASSES`]).
///
/// The derivation is a fallback for the records that predate the field, not a second
/// source of truth: a case that expects a failure phase exercises a `malformed` input; a
/// case carrying a tolerated difference exercises the reference's leniency; everything
/// else reads as `valid`. `boundary` and `unsupported` have no derivable signal at all,
/// which is precisely why the field exists — inference cannot represent a judgement.
fn input_class_of(case: &TestCase) -> &'static str {
    if let Some(declared) = case.input_class {
        return declared.as_str();
    }
    if case
        .operations
        .iter()
        .any(|op| op.expect_failure_phase.is_some())
    {
        "malformed"
    } else if !case.allowed_differences.is_empty() {
        "reference-lenient"
    } else {
        "valid"
    }
}

/// The input classes an operation's cases are expected to span (FR-040).
///
/// Every class, minus `reference-lenient` for an operation the pinned reference does not
/// implement: leniency is a difference between two implementations, and where there is
/// only one implementation the class does not exist to be exercised. Reporting it missing
/// there would demand a case that cannot be written, and a missing-list containing
/// impossible entries stops being read.
fn permitted_input_classes(operation: &str) -> Vec<&'static str> {
    let has_reference = crate::model::differential_substitution(operation).is_none();
    INPUT_CLASSES
        .iter()
        .copied()
        .filter(|class| has_reference || *class != InputClass::ReferenceLenient.as_str())
        .collect()
}

fn build_operations(registry: &Registry) -> OperationsReport {
    let model = ScenarioModel::new(&registry.scenario, &registry.applicability);
    let config_source_dimension = model.dimension("sdim-config-source");

    let mut operations = Vec::new();
    let Some(operation_dimension) = model.operation_dimension() else {
        return OperationsReport {
            schema_version: COVERAGE_REPORT_SCHEMA_VERSION,
            operations,
        };
    };

    for operation in &operation_dimension.values {
        let cases: Vec<&TestCase> = registry
            .cases
            .iter()
            .filter(|case| case_operations(case).contains(operation))
            .collect();

        let mut input_classes: IndexMap<String, usize> = INPUT_CLASSES
            .iter()
            .map(|class| ((*class).to_string(), 0))
            .collect();
        for case in &cases {
            if let Some(slot) = input_classes.get_mut(input_class_of(case)) {
                *slot += 1;
            }
        }

        let permitted: Vec<&str> = config_source_dimension
            .map(|d| model.permitted_values(operation, d))
            .unwrap_or_default();
        let mut config_sources: IndexMap<String, usize> = permitted
            .iter()
            .map(|value| ((*value).to_string(), 0))
            .collect();
        for case in &cases {
            if let Some(source) = case.scenario_context.get("sdim-config-source")
                && let Some(slot) = config_sources.get_mut(source)
            {
                *slot += 1;
            }
        }

        let mut channels: BTreeSet<String> = BTreeSet::new();
        for case in &cases {
            channels.extend(case.expected.iter().map(|e| e.channel.clone()));
            channels.extend(case.outcomes.iter().map(|o| o.channel.clone()));
        }

        let permitted = permitted_input_classes(operation);
        let substitution = crate::model::differential_substitution(operation);
        operations.push(OperationCoverage {
            operation: operation.clone(),
            cases: cases.len(),
            differential_available: substitution.is_none(),
            differential_substitution: substitution.map(str::to_string),
            missing_input_classes: input_classes
                .iter()
                .filter(|(class, count)| **count == 0 && permitted.contains(&class.as_str()))
                .map(|(class, _)| class.clone())
                .collect(),
            missing_config_sources: config_sources
                .iter()
                .filter(|(_, count)| **count == 0)
                .map(|(source, _)| source.clone())
                .collect(),
            input_classes,
            config_sources,
            channels: channels.into_iter().collect(),
        });
    }

    OperationsReport {
        schema_version: COVERAGE_REPORT_SCHEMA_VERSION,
        operations,
    }
}

/// The dotted observable paths a case compares on `channel`.
///
/// Two sources, both evidence that the comparison reaches the path: the leaf paths of a
/// `jsonSubset` assertion (what the case explicitly pins), and the path of an allowed
/// difference (a tolerance is only ever needed because the comparison surfaced a
/// difference there). An assertion with no path structure — `nonZero`, `contains` —
/// compares the channel wholesale and contributes no path.
fn compared_paths(case: &TestCase, channel: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for expectation in case.expected.iter().filter(|e| e.channel == channel) {
        if let Some(subset) = expectation
            .assertion
            .as_ref()
            .and_then(|a| a.get("jsonSubset"))
        {
            collect_json_paths(subset, String::new(), &mut out);
        }
    }
    let prefix = format!("{channel}.");
    for difference in &case.allowed_differences {
        if let Some(path) = difference.observable_path.strip_prefix(&prefix) {
            out.insert(path.to_string());
        }
    }
    out
}

/// Walk a `jsonSubset` value, emitting one dotted path per leaf.
fn collect_json_paths(value: &serde_json::Value, prefix: String, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_json_paths(child, path, out);
            }
        }
        _ => {
            if !prefix.is_empty() {
                out.insert(prefix);
            }
        }
    }
}

/// The US5 field ids a set of compared paths evidences.
fn denormalized_fields_for(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let lowered: Vec<String> = paths.iter().map(|p| p.to_ascii_lowercase()).collect();
    DENORMALIZED_FIELDS
        .iter()
        .filter(|(_, fragments)| {
            fragments
                .iter()
                .any(|fragment| lowered.iter().any(|path| path.contains(fragment)))
        })
        .map(|(field, _)| (*field).to_string())
        .collect()
}

fn build_observables(registry: &Registry) -> ObservablesReport {
    let mut channels = Vec::new();
    let mut field_cases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // Only cases something actually RUNS count toward the SC-005 three-case floor, the
    // same population `coverage::evaluate_obligations` gates on. Counting every registry
    // record let a legacy case whose residual has been closed — and whose carrier binary
    // is therefore deleted — satisfy the floor. That inverts the floor's entire purpose:
    // it exists because a channel carried by one case is one authoring mistake from
    // unobserved, and a case nothing executes observes nothing at all.
    let executable = crate::coverage::executable_case_ids(registry);
    for channel in &registry.channels {
        let mut cases = 0usize;
        let mut fields: BTreeSet<String> = BTreeSet::new();
        let mut covered: BTreeSet<String> = BTreeSet::new();
        for case in &registry.cases {
            let observes = case.expected.iter().any(|e| e.channel == channel.id)
                || case.outcomes.iter().any(|o| o.channel == channel.id);
            if !observes || !executable.contains(case.id.as_str()) {
                continue;
            }
            cases += 1;
            let paths = compared_paths(case, &channel.id);
            for field in denormalized_fields_for(&paths) {
                field_cases
                    .entry(field.clone())
                    .or_default()
                    .insert(case.id.clone());
                covered.insert(field);
            }
            fields.extend(paths);
        }
        channels.push(ChannelCoverage {
            channel: channel.id.clone(),
            cases,
            fields: fields.into_iter().collect(),
            denormalized_fields_covered: covered.into_iter().collect(),
        });
    }

    let denormalized_fields = DENORMALIZED_FIELDS
        .iter()
        .map(|(field, _)| {
            let by: Vec<String> = field_cases
                .get(*field)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();
            DenormalizedField {
                field: (*field).to_string(),
                covered: !by.is_empty(),
                by,
            }
        })
        .collect();

    let unscoped_normalization_rules =
        crate::conservation::check_normalization_rules(crate::conservation::NORMALIZATION_RULES)
            .into_iter()
            .map(|violation| violation.record)
            .collect();

    let channels_below_floor = channels
        .iter()
        .filter(|c| c.cases < CHANNEL_CASE_FLOOR)
        .count();

    ObservablesReport {
        schema_version: COVERAGE_REPORT_SCHEMA_VERSION,
        channels,
        denormalized_fields,
        unscoped_normalization_rules,
        summary: ObservablesSummary {
            channels_below_floor,
        },
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Pretty-print a report document to its canonical string: 2-space indent, LF endings,
/// trailing newline.
fn render_json<T: Serialize>(document: &T) -> String {
    let mut out = serde_json::to_string_pretty(document)
        .unwrap_or_else(|e| unreachable!("coverage report serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// `coverage-pairwise.md`, rendered from the same ordered model as its JSON.
pub fn render_pairwise_md(report: &PairwiseReport) -> String {
    let mut md = String::new();
    md.push_str("# Pairwise Combination Coverage\n\n");
    let s = &report.summary;
    md.push_str("| Bucket | Count |\n|--------|-------|\n");
    let _ = writeln!(md, "| valid (total obligations) | {} |", s.valid);
    let _ = writeln!(md, "| covered | {} |", s.covered);
    let _ = writeln!(md, "| waived | {} |", s.waived);
    let _ = writeln!(md, "| non-testable | {} |", s.non_testable);
    let _ = writeln!(md, "| gap | {} |", s.gap);
    let _ = writeln!(md, "| inactive-environment | {} |", s.inactive_environment);
    let _ = writeln!(md, "| **undispositioned** | **{}** |", s.undispositioned);
    md.push('\n');
    md.push_str("| Obligation kind | Count |\n|-----------------|-------|\n");
    for (kind, count) in &s.kinds {
        let _ = writeln!(md, "| {kind} | {count} |");
    }
    md.push('\n');

    md.push_str("## Per operation\n\n");
    md.push_str("| Operation | Applicable dimensions | Pairs | Covered | Excluded |\n");
    md.push_str("|-----------|-----------------------|-------|---------|----------|\n");
    for operation in &report.operations {
        let covered = operation
            .pairs
            .iter()
            .filter(|p| p.bucket == ObligationBucket::Covered.as_str())
            .count();
        let _ = writeln!(
            md,
            "| `{}` | {} | {} | {} | {} |",
            operation.operation,
            operation.applicable_dimensions.len(),
            operation.pairs.len(),
            covered,
            operation.excluded.len()
        );
    }
    md.push('\n');

    for operation in &report.operations {
        let _ = writeln!(md, "### `{}`\n", operation.operation);
        let _ = writeln!(
            md,
            "Applicable dimensions: {}\n",
            if operation.applicable_dimensions.is_empty() {
                "_none_".to_string()
            } else {
                operation
                    .applicable_dimensions
                    .iter()
                    .map(|d| format!("`{d}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        if !operation.excluded.is_empty() {
            md.push_str("Excluded by rule:\n\n");
            md.push_str("| Combination | Rule |\n|-------------|------|\n");
            for entry in &operation.excluded {
                let _ = writeln!(
                    md,
                    "| {} | `{}` |",
                    render_assignment(&entry.assignment),
                    entry.rule
                );
            }
            md.push('\n');
        }
        if operation.pairs.is_empty() {
            md.push_str("_No combination obligations._\n\n");
            continue;
        }
        md.push_str("| Obligation | Combination | Bucket | By |\n");
        md.push_str("|------------|-------------|--------|----|\n");
        for pair in &operation.pairs {
            let _ = writeln!(
                md,
                "| `{}` | {} | {} | {} |",
                pair.obligation,
                render_assignment(&pair.assignment),
                pair.bucket,
                if pair.by.is_empty() {
                    "—".to_string()
                } else {
                    pair.by
                        .iter()
                        .map(|id| format!("`{id}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
        }
        md.push('\n');
    }

    md.push_str("## Dead values\n\n");
    if report.dead_values.is_empty() {
        md.push_str("_None — every declared value appears in at least one valid combination._\n");
    } else {
        md.push_str("| Dimension | Value |\n|-----------|-------|\n");
        for dead in &report.dead_values {
            let _ = writeln!(md, "| `{}` | `{}` |", dead.dimension, dead.value);
        }
    }
    md
}

fn render_assignment(assignment: &IndexMap<String, String>) -> String {
    assignment
        .iter()
        .map(|(k, v)| format!("`{k}`=`{v}`"))
        .collect::<Vec<_>>()
        .join(" × ")
}

/// `coverage-triples.md`, rendered from the same ordered model as its JSON.
pub fn render_triples_md(report: &TriplesReport) -> String {
    let mut md = String::new();
    md.push_str("# High-Risk Triple Coverage\n\n");
    let s = &report.summary;
    md.push_str("| Bucket | Count |\n|--------|-------|\n");
    let _ = writeln!(md, "| selected | {} |", s.selected);
    let _ = writeln!(md, "| covered | {} |", s.covered);
    let _ = writeln!(md, "| gap | {} |", s.gap);
    let _ = writeln!(md, "| other (undispositioned) | {} |", s.other);
    md.push('\n');
    md.push_str(
        "A triple accepts only `case` or `gap` (FR-015): it is selected precisely because \
         interaction defects hide there, so an argument cannot stand in for evidence. V29 \
         rejects a rationale or a waiver on one.\n\n",
    );
    if report.triples.is_empty() {
        md.push_str("_No high-risk triples selected._\n");
        return md;
    }
    md.push_str("| Triple | Combination | Bucket | By |\n");
    md.push_str("|--------|-------------|--------|----|\n");
    for triple in &report.triples {
        let _ = writeln!(
            md,
            "| `{}` | {} | {} | {} |",
            triple.id,
            render_assignment(&triple.assignment),
            triple.bucket,
            render_list(&triple.by)
        );
    }
    md.push('\n');
    md.push_str("## Why each triple was selected\n\n");
    for triple in &report.triples {
        let _ = writeln!(md, "### `{}`\n", triple.id);
        let _ = writeln!(md, "- Obligation: `{}`", triple.obligation);
        let _ = writeln!(
            md,
            "- Combination: {}",
            render_assignment(&triple.assignment)
        );
        let _ = writeln!(md, "- Bucket: {}", triple.bucket);
        let _ = writeln!(md, "\n{}\n", triple.reason);
    }
    md
}

/// `coverage-operations.md`, rendered from the same ordered model as its JSON.
pub fn render_operations_md(report: &OperationsReport) -> String {
    let mut md = String::new();
    md.push_str("# Per-Operation Coverage\n\n");
    md.push_str(
        "| Operation | Cases | Channels | Missing input classes | Missing config sources |\n",
    );
    md.push_str(
        "|-----------|-------|----------|-----------------------|------------------------|\n",
    );
    for operation in &report.operations {
        let _ = writeln!(
            md,
            "| `{}` | {} | {} | {} | {} |",
            operation.operation,
            operation.cases,
            operation.channels.len(),
            render_list(&operation.missing_input_classes),
            render_list(&operation.missing_config_sources)
        );
    }
    md.push('\n');
    for operation in &report.operations {
        let _ = writeln!(md, "### `{}`\n", operation.operation);
        let _ = writeln!(md, "Cases: {}\n", operation.cases);
        if let Some(substitution) = &operation.differential_substitution {
            let _ = writeln!(
                md,
                "**No runnable differential against the pinned reference** — {substitution}.\n"
            );
        }
        md.push_str("| Input class | Cases |\n|-------------|-------|\n");
        for (class, count) in &operation.input_classes {
            let _ = writeln!(md, "| {class} | {count} |");
        }
        md.push('\n');
        md.push_str("| Config source | Cases |\n|---------------|-------|\n");
        for (source, count) in &operation.config_sources {
            let _ = writeln!(md, "| `{source}` | {count} |");
        }
        md.push('\n');
        let _ = writeln!(md, "Channels: {}\n", render_list(&operation.channels));
    }
    md
}

/// `coverage-observables.md`, rendered from the same ordered model as its JSON.
pub fn render_observables_md(report: &ObservablesReport) -> String {
    let mut md = String::new();
    md.push_str("# Per-Observable Coverage\n\n");
    let _ = writeln!(
        md,
        "Channels compared by fewer than {CHANNEL_CASE_FLOOR} cases: **{}**\n",
        report.summary.channels_below_floor
    );
    md.push_str("| Channel | Cases | Compared fields | Below floor |\n");
    md.push_str("|---------|-------|-----------------|-------------|\n");
    for channel in &report.channels {
        let _ = writeln!(
            md,
            "| `{}` | {} | {} | {} |",
            channel.channel,
            channel.cases,
            channel.fields.len(),
            if channel.cases < CHANNEL_CASE_FLOOR {
                "yes"
            } else {
                "no"
            }
        );
    }
    md.push('\n');
    md.push_str("## De-normalized fields (US5)\n\n");
    md.push_str("| Field | Covered | By |\n|-------|---------|----|\n");
    for field in &report.denormalized_fields {
        let _ = writeln!(
            md,
            "| `{}` | {} | {} |",
            field.field,
            if field.covered { "yes" } else { "no" },
            render_list(&field.by)
        );
    }
    md.push('\n');
    md.push_str("## Unscoped normalization rules\n\n");
    if report.unscoped_normalization_rules.is_empty() {
        md.push_str("_None — every normalization rule is named, scoped, and justified (V24)._\n");
    } else {
        for rule in &report.unscoped_normalization_rules {
            let _ = writeln!(md, "- `{rule}`");
        }
    }
    md
}

fn render_list(items: &[String]) -> String {
    if items.is_empty() {
        "—".to_string()
    } else {
        items
            .iter()
            .map(|i| format!("`{i}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Write every report family into `dir` atomically, returning the written paths in a
/// deterministic order.
pub fn write_coverage_reports(
    dir: &Path,
    reports: &CoverageReports,
) -> std::io::Result<Vec<PathBuf>> {
    let artifacts: Vec<(&str, String)> = vec![
        ("coverage-pairwise.json", render_json(&reports.pairwise)),
        (
            "coverage-pairwise.md",
            render_pairwise_md(&reports.pairwise),
        ),
        ("coverage-triples.json", render_json(&reports.triples)),
        ("coverage-triples.md", render_triples_md(&reports.triples)),
        ("coverage-operations.json", render_json(&reports.operations)),
        (
            "coverage-operations.md",
            render_operations_md(&reports.operations),
        ),
        (
            "coverage-observables.json",
            render_json(&reports.observables),
        ),
        (
            "coverage-observables.md",
            render_observables_md(&reports.observables),
        ),
    ];
    let mut written = Vec::new();
    for (name, contents) in artifacts {
        let path = dir.join(name);
        crate::atomic_write(&path, &contents)?;
        written.push(path);
    }
    Ok(written)
}
