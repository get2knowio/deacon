//! Continuous-integration **lanes** and the derived **execution-unit denominator**
//! (026-continuous-conformance-certification, US3; data-model.md §1–§2).
//!
//! A lane is a named execution context with a declared inclusion rule. The point of
//! modelling lanes as data rather than as workflow YAML is that "every unit this system
//! owns runs somewhere" becomes a checkable claim (**V34**) instead of a thing a reviewer
//! has to notice.
//!
//! ## Why the denominator is derived and not authored
//!
//! [`derive_execution_units`] enumerates the unit set from the same sources the system
//! already uses to discover its own work. It is deliberately NOT a hand-authored list:
//! a unit omitted from such a list would satisfy "every unit is assigned to a lane" while
//! being covered by nothing, which inverts the check into a rubber stamp (FR-003a). The
//! only way to make a unit disappear from the denominator is to delete the unit.
//!
//! ## Two selection rules, for two different failure modes
//!
//! | Unit kind | Selected by | Because |
//! |---|---|---|
//! | validation class, program | an explicit allow-list (FR-002) | a glob silently captures a new binary or silently drops a renamed one — the mistake the parity and discovery nextest profiles have each documented making |
//! | case | a derived predicate over `oracleType` × `resourceGroup` (FR-002a) | an id list would need editing on every added case, and a forgotten edit leaves the case selected by nothing |
//!
//! The asymmetry is not an inconsistency. A predicate may silently *capture* a new case,
//! which is intended; what it must never do is silently *drop* one, and that is what the
//! partition check in [`check_lanes`] proves.
//!
//! ## Program scope
//!
//! The program denominator is the set of test programs that reference this crate or
//! `parity_harness` — the programs the conformance system owns. That is itself a derived
//! predicate, so a new conformance program joins the denominator automatically while an
//! ordinary `deacon` integration test does not. The general test suite is governed by the
//! pre-existing continuous-integration lanes, which this feature does not redefine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::load::Registry;
use crate::model::{OracleType, ResourceGroup};
use crate::validate::Violation;

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// The `lanes.json` document: `{ "schemaVersion", "records" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaneFile {
    pub schema_version: u32,
    pub records: Vec<Lane>,
}

/// One `lane-` record (data-model.md §1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lane {
    pub id: String,
    pub display_name: String,
    pub trigger: Trigger,
    /// Whether a failure in this lane blocks. FR-015/FR-019 require `false` for the
    /// nightly and canary lanes; V34 enforces it.
    pub blocking: bool,
    #[serde(default)]
    pub preconditions: Vec<Precondition>,
    /// The nextest profiles this lane runs. Empty for a lane that runs no test binary
    /// (release certification validates data and reads a manifest).
    #[serde(default)]
    pub nextest_profiles: Vec<String>,
    /// Whether this lane may write to the record. MUST be `false` for every lane
    /// (FR-016, FR-020). The field exists so the constraint is *stated* and testable
    /// rather than merely absent — a future lane that wants to write has to change a
    /// value a reviewer sees.
    pub may_write_record: bool,
    pub includes: Inclusion,
    pub excludes: Exclusion,
}

/// When a lane runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trigger {
    PullRequest,
    Nightly,
    Weekly,
    Invoked,
    Release,
}

/// A capability a lane requires. FR-004: when one is unavailable the lane MUST fail,
/// never skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Precondition {
    ContainerEngine,
    ReferenceOracle,
    Network,
}

/// What a lane selects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inclusion {
    #[serde(default)]
    pub validation_classes: Vec<String>,
    #[serde(default)]
    pub programs: Vec<String>,
    /// `None` means "no case belongs to this lane".
    #[serde(default)]
    pub case_predicate: Option<CasePredicate>,
    #[serde(default)]
    pub snapshot_replay: bool,
}

/// What a lane deliberately leaves out, and why (FR-005).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exclusion {
    /// Required prose. A lane that excludes units without saying why produces a green
    /// status a reader cannot interpret.
    pub rationale: String,
    #[serde(default)]
    pub case_predicate: Option<CasePredicate>,
}

/// A predicate over existing case properties (FR-002a, research D9). Never an id list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CasePredicate {
    #[serde(default)]
    pub oracle_types: Vec<OracleType>,
    #[serde(default)]
    pub resource_groups: Vec<ResourceGroup>,
}

impl CasePredicate {
    /// Whether this predicate selects a case with the given derived membership.
    fn matches(&self, m: &CaseMembership) -> bool {
        self.oracle_types.contains(&m.oracle_type)
            && self.resource_groups.contains(&m.resource_group)
    }
}

// ---------------------------------------------------------------------------
// Case → lane membership (T010, research D9)
// ---------------------------------------------------------------------------

/// The two properties that decide which lane a case belongs to, both already recorded on
/// the case (research D9). No new per-case field is introduced: a `lane` field would be a
/// second source of truth that could contradict `oracleType`, and the contradiction would
/// be silent — a case marked oracle-free but typed `live-differential` would fail inside
/// the runner rather than at load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseMembership {
    pub oracle_type: OracleType,
    pub resource_group: ResourceGroup,
    /// Whether evaluating this case requires the pinned reference implementation.
    pub needs_oracle: bool,
    /// Whether evaluating this case requires a container engine.
    pub needs_container: bool,
}

/// Derive lane membership for a declarative case. Returns `None` for a legacy case (no
/// `oracleType`), which is carried by its own binary and is assigned as a *program* unit
/// rather than a case unit.
pub fn case_lane_membership(case: &crate::model::TestCase) -> Option<CaseMembership> {
    let oracle_type = case.oracle_type?;
    let resource_group = case.resource_group.unwrap_or(ResourceGroup::None);
    Some(CaseMembership {
        oracle_type,
        resource_group,
        // Only a live differential resolves the reference. `spec-expectation` compares to
        // a declared assertion, `snapshot` to committed evidence, and
        // `invariant-metamorphic` to a relationship between deacon's own runs.
        needs_oracle: matches!(oracle_type, OracleType::LiveDifferential),
        needs_container: matches!(
            resource_group,
            ResourceGroup::DockerShared | ResourceGroup::DockerExclusive
        ),
    })
}

// ---------------------------------------------------------------------------
// Execution units (T009, data-model.md §2)
// ---------------------------------------------------------------------------

/// The kind of an execution unit — the finest granularity for which a lane reports an
/// independent outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitKind {
    ValidationClass,
    Case,
    Program,
    SnapshotReplay,
}

/// One derived execution unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionUnit {
    pub id: String,
    pub kind: UnitKind,
    /// The underlying identity: the class code, case id, program name, or
    /// `<os-arch>/<case-id>`.
    pub subject: String,
}

/// Every registry violation class this crate can emit, plus the schema pseudo-class.
///
/// Hand-maintained in exactly one place and cross-checked against `validate.rs` by
/// [`crate::validate`]'s own tests. `V25` is absent deliberately: it was retired in 023
/// (a permanent baseline-provenance gate would forbid ever retiring the machinery the
/// migration exists to retire), so listing it would put a unit in the denominator that no
/// lane could ever run.
pub const REGISTRY_VALIDATION_CLASSES: &[&str] = &[
    "SCHEMA", "V1", "V2", "V3", "V4", "V5", "V6", "V7", "V8", "V9", "V10", "V11", "V12", "V13",
    "V14", "V15", "V16", "V17", "V18", "V19", "V20", "V21", "V22", "V23", "V24", "V26", "V27",
    "V28", "V29", "V30", "V31", "V32", "V34", "V35", "V36",
];

/// Every discovery violation class (data-model.md §2: **both** enumerations contribute to
/// the denominator, because a D-class is as much an independent outcome as a V-class).
pub const DISCOVERY_VALIDATION_CLASSES: &[&str] = &["D1", "D2", "D3", "D4", "D5", "D6"];

/// Derive the complete execution-unit denominator (FR-003a).
///
/// Four sources, all of them the system's own enumerations:
///
/// 1. **validation classes** — [`REGISTRY_VALIDATION_CLASSES`] + [`DISCOVERY_VALIDATION_CLASSES`];
/// 2. **cases** — every declarative case in the registry;
/// 3. **programs** — every conformance-owned test program under `tests_dir`;
/// 4. **snapshot replay targets** — every `<os-arch>/<case-id>/` under `snapshots_dir`.
///
/// `tests_dir` and `snapshots_dir` that do not exist contribute nothing, so a fixture
/// registry derives a denominator over its own data without needing a source tree.
pub fn derive_execution_units(
    registry: &Registry,
    tests_dir: &Path,
    snapshots_dir: &Path,
) -> Vec<ExecutionUnit> {
    let mut units = Vec::new();

    for class in REGISTRY_VALIDATION_CLASSES
        .iter()
        .chain(DISCOVERY_VALIDATION_CLASSES.iter())
    {
        units.push(ExecutionUnit {
            id: format!("unit-vcls-{class}"),
            kind: UnitKind::ValidationClass,
            subject: (*class).to_string(),
        });
    }

    for case in &registry.cases {
        if case.oracle_type.is_some() {
            units.push(ExecutionUnit {
                id: format!("unit-case-{}", case.id),
                kind: UnitKind::Case,
                subject: case.id.clone(),
            });
        }
    }

    for program in derive_conformance_programs(tests_dir) {
        units.push(ExecutionUnit {
            id: format!("unit-prog-{program}"),
            kind: UnitKind::Program,
            subject: program,
        });
    }

    for target in derive_snapshot_targets(snapshots_dir) {
        units.push(ExecutionUnit {
            id: format!("unit-snap-{target}"),
            kind: UnitKind::SnapshotReplay,
            subject: target,
        });
    }

    units.sort_by(|a, b| (a.kind, &a.subject).cmp(&(b.kind, &b.subject)));
    units
}

/// The conformance-owned test programs under `tests_dir`: those whose source references
/// this crate or `parity_harness`.
///
/// Deriving membership this way rather than listing names is what keeps FR-003a true for
/// programs. A new conformance test binary joins the denominator the moment it uses the
/// crate; an ordinary `deacon` integration test never enters it, so this feature does not
/// silently claim authority over the whole test suite.
pub fn derive_conformance_programs(tests_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(tests_dir) else {
        return Vec::new();
    };
    let mut programs = BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        if source.contains("deacon_conformance") || source.contains("parity_harness") {
            programs.insert(stem.to_string());
        }
    }
    programs.into_iter().collect()
}

/// The committed snapshot replay targets: `<os-arch>/<case-id>` for every directory with
/// a `provenance.json`.
pub fn derive_snapshot_targets(snapshots_dir: &Path) -> Vec<String> {
    let Ok(platforms) = std::fs::read_dir(snapshots_dir) else {
        return Vec::new();
    };
    let mut targets = BTreeSet::new();
    for platform in platforms.flatten() {
        if !platform.path().is_dir() {
            continue;
        }
        let Some(platform_name) = platform.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(cases) = std::fs::read_dir(platform.path()) else {
            continue;
        };
        for case in cases.flatten() {
            if !case.path().join("provenance.json").is_file() {
                continue;
            }
            if let Some(case_name) = case.file_name().to_str() {
                targets.insert(format!("{platform_name}/{case_name}"));
            }
        }
    }
    targets.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load `lanes.json` from a lane root. A missing root yields no lanes, so a fixture
/// registry validates without one.
pub fn load_lanes(lanes_dir: &Path) -> Result<Vec<Lane>, LaneLoadError> {
    let path = lanes_dir.join("lanes.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| LaneLoadError {
        path: path.display().to_string(),
        cause: e.to_string(),
    })?;
    let file: LaneFile = serde_json::from_str(&raw).map_err(|e| LaneLoadError {
        path: path.display().to_string(),
        cause: e.to_string(),
    })?;
    Ok(file.records)
}

/// A `lanes.json` that could not be read or parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "lane file `{path}` could not be loaded: {cause}. Remedy: fix the record — \
     `lanes.json` is strict JSON and rejects unknown fields at load."
)]
pub struct LaneLoadError {
    pub path: String,
    pub cause: String,
}

// ---------------------------------------------------------------------------
// V34 (T068)
// ---------------------------------------------------------------------------

/// The lanes that FR-015 / FR-019 require to be non-blocking.
const MUST_BE_NON_BLOCKING: &[&str] = &["lane-nightly-stable", "lane-canary"];

/// **V34 — lane integrity** (026, US3; data-model.md §8).
///
/// | Sub-case | Guards |
/// |---|---|
/// | zero-assignment | a derived unit no lane selects (FR-003) |
/// | unknown reference | a lane naming a validation class or program that does not exist |
/// | case partition | `includes`/`excludes` predicates that overlap or leave a remainder (FR-002a) |
/// | blocking | a lane FR-015/FR-019 requires non-blocking that declares `blocking: true` |
/// | write | any lane declaring `mayWriteRecord: true` (FR-016, FR-020) |
/// | profile drift | a declared `nextestProfile` whose filter disagrees with the declared programs |
///
/// `profiles` maps profile name → the binaries that profile selects, parsed from
/// `.config/nextest.toml` by the caller. Passing `None` skips only the profile-drift
/// sub-case, so a fixture registry with no nextest configuration still gets every other
/// check.
pub fn check_lanes(
    lanes: &[Lane],
    units: &[ExecutionUnit],
    memberships: &BTreeMap<String, CaseMembership>,
    profiles: Option<&BTreeMap<String, crate::validate::ProfileFilter>>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    if lanes.is_empty() {
        return violations;
    }

    let known_classes: BTreeSet<&str> = REGISTRY_VALIDATION_CLASSES
        .iter()
        .chain(DISCOVERY_VALIDATION_CLASSES.iter())
        .copied()
        .collect();
    let known_programs: BTreeSet<&str> = units
        .iter()
        .filter(|u| u.kind == UnitKind::Program)
        .map(|u| u.subject.as_str())
        .collect();

    // -- per-lane well-formedness ------------------------------------------
    for lane in lanes {
        if lane.may_write_record {
            violations.push(Violation::v34(
                &lane.id,
                "declares `mayWriteRecord: true`. No lane may write to the record \
                 (FR-016, FR-020); evidence is recorded only through the reviewed record path.",
            ));
        }
        if MUST_BE_NON_BLOCKING.contains(&lane.id.as_str()) && lane.blocking {
            violations.push(Violation::v34(
                &lane.id,
                "declares `blocking: true`, but this lane must be non-blocking \
                 (FR-015/FR-019) — its status reflects whether it ran, not what it found.",
            ));
        }
        if lane.excludes.rationale.trim().is_empty() {
            violations.push(Violation::v34(
                &lane.id,
                "declares an empty `excludes.rationale`. A lane that excludes units \
                 without saying why produces a green status a reader cannot interpret (FR-005).",
            ));
        }
        for class in &lane.includes.validation_classes {
            if !known_classes.contains(class.as_str()) {
                violations.push(Violation::v34(
                    &lane.id,
                    format!(
                        "names validation class `{class}`, which no validator emits. \
                         Remedy: use a declared class, or remove the reference."
                    ),
                ));
            }
        }
        for program in &lane.includes.programs {
            if !known_programs.contains(program.as_str()) {
                violations.push(Violation::v34(
                    &lane.id,
                    format!(
                        "names program `{program}`, which is not a conformance-owned test \
                         program. Remedy: create it, or remove the reference."
                    ),
                ));
            }
        }
    }

    // -- case partition (FR-002a) ------------------------------------------
    for (case_id, membership) in memberships {
        let including: Vec<&str> = lanes
            .iter()
            .filter(|l| {
                l.includes
                    .case_predicate
                    .as_ref()
                    .is_some_and(|p| p.matches(membership))
            })
            .map(|l| l.id.as_str())
            .collect();
        if including.is_empty() {
            violations.push(Violation::v34(
                case_id,
                format!(
                    "matches no lane's `includes.casePredicate` (oracleType `{:?}`, \
                     resourceGroup `{:?}`). A case selected by nothing runs nowhere — \
                     the predicate must partition the case space (FR-002a).",
                    membership.oracle_type, membership.resource_group
                ),
            ));
        } else if including.len() > 1 {
            violations.push(Violation::v34(
                case_id,
                format!(
                    "matches {} lanes' `includes.casePredicate` ({}). The predicates must \
                     partition the case space with no overlap (FR-002a).",
                    including.len(),
                    including.join(", ")
                ),
            ));
        }
        for lane in lanes {
            let included = lane
                .includes
                .case_predicate
                .as_ref()
                .is_some_and(|p| p.matches(membership));
            let excluded = lane
                .excludes
                .case_predicate
                .as_ref()
                .is_some_and(|p| p.matches(membership));
            if included && excluded {
                violations.push(Violation::v34(
                    &lane.id,
                    format!(
                        "both includes and excludes case `{case_id}`. A case cannot be \
                         simultaneously selected and deliberately left out."
                    ),
                ));
            }
        }
    }

    // -- full assignment (FR-003) ------------------------------------------
    for unit in units {
        let assigned = lanes.iter().any(|lane| match unit.kind {
            UnitKind::ValidationClass => lane.includes.validation_classes.contains(&unit.subject),
            UnitKind::Program => lane.includes.programs.contains(&unit.subject),
            UnitKind::SnapshotReplay => lane.includes.snapshot_replay,
            UnitKind::Case => memberships
                .get(&unit.subject)
                .and_then(|m| lane.includes.case_predicate.as_ref().map(|p| p.matches(m)))
                .unwrap_or(false),
        });
        if !assigned {
            violations.push(Violation::v34(
                &unit.id,
                format!(
                    "is assigned to zero lanes. Every execution unit must run in at least \
                     one lane (FR-003); an unassigned {:?} unit is covered by nothing.",
                    unit.kind
                ),
            ));
        }
    }

    // -- profile drift ------------------------------------------------------
    //
    // The check a lane taxonomy is worth nothing without: a lane may declare a program its
    // profile does not actually run. Note the asymmetry between filter forms — an
    // exclusion filter (`not (…)`) runs everything it does not name, an allow-list runs
    // only what it names — which is why [`ProfileFilter::selects`] exists rather than a
    // bare set membership test.
    if let Some(profiles) = profiles {
        for lane in lanes {
            for profile_name in &lane.nextest_profiles {
                if !profiles.contains_key(profile_name) {
                    violations.push(Violation::v34(
                        &lane.id,
                        format!(
                            "declares nextest profile `{profile_name}`, which is not \
                             defined in `.config/nextest.toml`."
                        ),
                    ));
                }
            }
            for program in &lane.includes.programs {
                // A lane may span profiles (the nightly lane runs both `parity` and
                // `discovery`), so a program need only be carried by ONE of them.
                let carried = lane
                    .nextest_profiles
                    .iter()
                    .filter_map(|p| profiles.get(p))
                    .any(|filter| filter.selects(program));
                if !carried {
                    violations.push(Violation::v34(
                        &lane.id,
                        format!(
                            "declares program `{program}`, which none of its nextest \
                             profiles ({}) selects. Remedy: add an explicit `binary(=…)` \
                             clause to the profile's `default-filter` — never a glob, which \
                             would silently capture an unintended binary (FR-002).",
                            lane.nextest_profiles.join(", ")
                        ),
                    ));
                }
            }
        }
    }

    violations
}

/// Build the `case-id → membership` map a [`check_lanes`] call needs.
pub fn case_memberships(registry: &Registry) -> BTreeMap<String, CaseMembership> {
    registry
        .cases
        .iter()
        .filter_map(|c| case_lane_membership(c).map(|m| (c.id.clone(), m)))
        .collect()
}

// ---------------------------------------------------------------------------
// Reporting (T071)
// ---------------------------------------------------------------------------

/// The per-lane ran/excluded breakdown `lane report` renders (FR-005).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneReport {
    pub schema_version: u32,
    pub lanes: Vec<LaneReportEntry>,
    /// Units assigned to zero lanes. Must be empty; V34 reports each one.
    pub unassigned: Vec<String>,
}

/// One lane's entry in the report.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneReportEntry {
    pub id: String,
    pub display_name: String,
    pub trigger: Trigger,
    pub blocking: bool,
    pub preconditions: Vec<Precondition>,
    pub nextest_profiles: Vec<String>,
    /// The unit ids this lane selects, sorted.
    pub selected: Vec<String>,
    /// The unit ids this lane deliberately leaves out, sorted — stated, never implied by
    /// omission (FR-005).
    pub excluded: Vec<String>,
    pub exclusion_rationale: String,
}

/// Build the deterministic lane report.
pub fn build_lane_report(
    lanes: &[Lane],
    units: &[ExecutionUnit],
    memberships: &BTreeMap<String, CaseMembership>,
) -> LaneReport {
    let selects = |lane: &Lane, unit: &ExecutionUnit| -> bool {
        match unit.kind {
            UnitKind::ValidationClass => lane.includes.validation_classes.contains(&unit.subject),
            UnitKind::Program => lane.includes.programs.contains(&unit.subject),
            UnitKind::SnapshotReplay => lane.includes.snapshot_replay,
            UnitKind::Case => memberships
                .get(&unit.subject)
                .and_then(|m| lane.includes.case_predicate.as_ref().map(|p| p.matches(m)))
                .unwrap_or(false),
        }
    };

    let entries = lanes
        .iter()
        .map(|lane| {
            let mut selected = Vec::new();
            let mut excluded = Vec::new();
            for unit in units {
                if selects(lane, unit) {
                    selected.push(unit.id.clone());
                } else {
                    excluded.push(unit.id.clone());
                }
            }
            selected.sort();
            excluded.sort();
            LaneReportEntry {
                id: lane.id.clone(),
                display_name: lane.display_name.clone(),
                trigger: lane.trigger,
                blocking: lane.blocking,
                preconditions: lane.preconditions.clone(),
                nextest_profiles: lane.nextest_profiles.clone(),
                selected,
                excluded,
                exclusion_rationale: lane.excludes.rationale.clone(),
            }
        })
        .collect();

    let mut unassigned: Vec<String> = units
        .iter()
        .filter(|u| !lanes.iter().any(|l| selects(l, u)))
        .map(|u| u.id.clone())
        .collect();
    unassigned.sort();

    LaneReport {
        schema_version: 1,
        lanes: entries,
        unassigned,
    }
}

/// Render the lane report as deterministic Markdown — no timestamps, no absolute paths.
pub fn render_lane_report_md(report: &LaneReport) -> String {
    let mut out = String::from("# Lane report\n\n");
    out.push_str("| Lane | Trigger | Blocking | Preconditions | Selected | Excluded |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for lane in &report.lanes {
        out.push_str(&format!(
            "| `{}` | {:?} | {} | {} | {} | {} |\n",
            lane.id,
            lane.trigger,
            lane.blocking,
            if lane.preconditions.is_empty() {
                "none".to_string()
            } else {
                lane.preconditions
                    .iter()
                    .map(|p| format!("{p:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            lane.selected.len(),
            lane.excluded.len()
        ));
    }
    out.push_str("\n## Deliberate exclusions\n\n");
    for lane in &report.lanes {
        out.push_str(&format!(
            "### `{}`\n\n{}\n\n",
            lane.id, lane.exclusion_rationale
        ));
    }
    out.push_str("## Unassigned units\n\n");
    if report.unassigned.is_empty() {
        out.push_str("None — every execution unit runs in at least one lane.\n");
    } else {
        for id in &report.unassigned {
            out.push_str(&format!("- `{id}`\n"));
        }
    }
    out
}

/// A skeleton lane record for `lane scaffold`. Every field a human must decide carries
/// the `UNREVIEWED` sentinel, which the loader rejects — a scaffold cannot be committed
/// unedited.
pub fn scaffold_lane(for_unit: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "UNREVIEWED",
        "displayName": "UNREVIEWED",
        "trigger": "UNREVIEWED",
        "blocking": "UNREVIEWED",
        "preconditions": [],
        "nextestProfiles": ["UNREVIEWED"],
        "mayWriteRecord": false,
        "includes": {
            "validationClasses": [],
            "programs": [for_unit],
            "casePredicate": null,
            "snapshotReplay": false
        },
        "excludes": {
            "rationale": "UNREVIEWED",
            "casePredicate": null
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn membership(oracle: OracleType, group: ResourceGroup) -> CaseMembership {
        CaseMembership {
            oracle_type: oracle,
            resource_group: group,
            needs_oracle: matches!(oracle, OracleType::LiveDifferential),
            needs_container: matches!(
                group,
                ResourceGroup::DockerShared | ResourceGroup::DockerExclusive
            ),
        }
    }

    #[test]
    fn live_differential_needs_the_oracle_and_others_do_not() {
        assert!(membership(OracleType::LiveDifferential, ResourceGroup::None).needs_oracle);
        assert!(!membership(OracleType::SpecExpectation, ResourceGroup::None).needs_oracle);
        assert!(!membership(OracleType::Snapshot, ResourceGroup::None).needs_oracle);
        assert!(!membership(OracleType::InvariantMetamorphic, ResourceGroup::None).needs_oracle);
    }

    #[test]
    fn docker_groups_need_a_container_engine() {
        assert!(
            membership(OracleType::SpecExpectation, ResourceGroup::DockerShared).needs_container
        );
        assert!(
            membership(OracleType::SpecExpectation, ResourceGroup::DockerExclusive).needs_container
        );
        assert!(!membership(OracleType::SpecExpectation, ResourceGroup::FsHeavy).needs_container);
        assert!(!membership(OracleType::SpecExpectation, ResourceGroup::None).needs_container);
    }

    #[test]
    fn the_retired_class_is_absent_from_the_denominator() {
        // V25 was retired in 023. Listing it would put a unit in the denominator that no
        // lane could ever run, so every lane check would fail permanently.
        assert!(!REGISTRY_VALIDATION_CLASSES.contains(&"V25"));
    }

    #[test]
    fn scaffold_carries_the_loader_rejecting_sentinel() {
        let skeleton = scaffold_lane("unit-prog-example");
        assert_eq!(skeleton["id"], "UNREVIEWED");
        // A scaffold must not deserialize: it is a starting point for a human, not a
        // record. `trigger` is a closed enum, so `"UNREVIEWED"` cannot parse.
        let parsed: Result<Lane, _> = serde_json::from_value(skeleton);
        assert!(parsed.is_err(), "scaffold must not round-trip into a Lane");
    }
}
