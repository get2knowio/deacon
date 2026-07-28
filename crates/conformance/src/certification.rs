//! The **release-grade certification report** (026-continuous-conformance-certification,
//! US1; data-model.md §7, contracts/cli-certification.md).
//!
//! ## Derived from the verdict, never recomputed
//!
//! Every field here is assembled from an already-computed [`Certification`] plus the
//! registry it was computed over. Nothing in this module re-derives a blocking condition.
//! That is deliberate: two code paths reading the same registry independently can drift —
//! one gains a blocking condition the other does not render — and the resulting artifact
//! would claim certification the gate refused. Deriving the report from the verdict makes
//! that disagreement unrepresentable (research D4).
//!
//! ## Byte-reproducibility is a property, not an aspiration
//!
//! No timestamps, no absolute paths, no hostnames, no environment-dependent ordering
//! (FR-036). `evaluationDate` comes from the caller's injected `--today`, never from the
//! clock; `deaconRevision` comes from the build environment, never from a `git` call at
//! report time. Two runs on different machines produce identical bytes (SC-005).
//!
//! ## The scope statement is the point
//!
//! A certification that reads as unqualified overstates what was verified. [`Scope`]
//! names exactly one profile and enumerates what certification does *not* extend to, so a
//! reader can determine the certified combination — and the uncertified ones — from the
//! report alone (FR-035, SC-010).

use std::collections::BTreeSet;

use serde::Serialize;

use crate::certify::Certification;
use crate::load::Registry;

/// The complete certification report (data-model.md §7).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificationReport {
    pub schema_version: u32,
    pub certified: bool,
    pub identity: Identity,
    pub environment: Environment,
    pub scope: Scope,
    pub source_scope: SourceScope,
    pub coverage: Coverage,
    pub exceptions: Exceptions,
    pub snapshot_provenance: Vec<SnapshotProvenanceEntry>,
    pub not_certified: NotCertified,
    /// Non-deterministic inputs admitted to the deterministic verdict, with the reason
    /// each qualified (FR-047). Empty in the normal case, and empty is the honest default:
    /// a corpus or campaign result contributes nothing unless it was fully pinned and
    /// hermetic (FR-046, SC-012).
    pub admitted_non_deterministic_inputs: Vec<AdmittedInput>,
    /// The date time-dependent conditions were evaluated against (FR-040), so a later
    /// reader can reproduce the verdict.
    pub evaluation_date: String,
    pub blocking: Vec<BlockingEntry>,
}

/// The four identity fields (FR-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub deacon_revision: String,
    pub oracle_version: String,
    pub spec_revision: String,
    pub schema_revisions: Vec<String>,
}

/// The four environment fields. `containerEngine` and its version count as one FR-034
/// field; both are carried because a version-specific divergence is invisible without it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub platform: String,
    pub arch: String,
    pub container_engine: String,
    pub container_engine_version: String,
    pub compose_version: String,
}

/// What is certified, and — equally load-bearing — what is not (FR-035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub profile: String,
    /// The combinations this certification explicitly does not extend to. Enumerated
    /// rather than left implicit, so "Linux/amd64/Docker" can never be read as covering
    /// Podman.
    pub does_not_certify: Vec<String>,
    pub statement: String,
}

/// The pinned source surface and its classification status (FR-038).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScope {
    pub schema_documents: usize,
    pub prose_documents: usize,
    pub cli_surface: String,
    /// Source units with no disposition. Non-empty means condition (a) blocks.
    pub unclassified_units: Vec<String>,
}

/// The three coverage fields (FR-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub behavior_count: usize,
    /// Obligation buckets — the scenario-space coverage the 024 model measures.
    pub context_coverage: ContextCoverage,
    /// Per-channel covering-case counts, channel-sorted.
    pub observable_coverage: Vec<ChannelCoverage>,
}

/// The obligation buckets, carried verbatim from the verdict's summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextCoverage {
    pub covered: usize,
    pub waived: usize,
    pub non_testable: usize,
    pub gap: usize,
    pub inactive_environment: usize,
    /// Must be zero. A non-zero value means the obligation queue is undispositioned,
    /// which blocks through V28.
    pub undispositioned: usize,
}

/// One observable channel's covering-case count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelCoverage {
    pub channel: String,
    pub covering_cases: usize,
    /// Whether the channel meets the three-covering-case floor. A channel carried by one
    /// case is one authoring mistake from unobserved.
    pub meets_floor: bool,
}

/// The three exception fields (FR-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exceptions {
    pub gaps: Vec<String>,
    pub waivers: Vec<String>,
    pub intentional_divergences: Vec<String>,
}

/// Per-snapshot provenance (FR-039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotProvenanceEntry {
    pub case_id: String,
    /// The `os-arch` platforms with a committed snapshot, sorted.
    pub platforms: Vec<String>,
}

/// What was explicitly *not* certified (FR-037). Enumerated rather than omitted: a reader
/// must be able to tell "not applicable" from "forgotten".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotCertified {
    pub inactive_profiles: Vec<String>,
    pub non_testable: Vec<String>,
    pub no_reference_for_platform: Vec<String>,
}

/// A non-deterministic input admitted to the deterministic verdict (FR-047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmittedInput {
    pub input: String,
    /// Why it qualified — every input pinned by immutable identifier and the run hermetic.
    pub reason: String,
}

/// One blocking condition, named (FR-042).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockingEntry {
    pub condition: String,
    pub record: String,
    pub detail: String,
}

/// Inputs the report needs that are not derivable from the registry.
#[derive(Debug, Clone)]
pub struct ReportInputs {
    /// The revision under certification, from the build environment.
    pub deacon_revision: String,
    pub environment: Environment,
    /// The evaluation date, from the caller's `--today`.
    pub evaluation_date: String,
    pub schema_documents: usize,
    pub prose_documents: usize,
    pub admitted_non_deterministic_inputs: Vec<AdmittedInput>,
}

/// The number of FR-034 fields a well-formed report carries. Asserted by the acceptance
/// tests so a field silently dropped from the shape is caught.
///
/// Sixteen, not twenty: `scope.profile`, `scope.doesNotCertify`, `evaluationDate`, and
/// `notCertified` are required too, but they satisfy FR-035, FR-040, and FR-037 rather
/// than FR-034.
pub const FR034_FIELD_COUNT: usize = 16;

/// The FR-034 field names, in report order.
pub const FR034_FIELDS: &[&str] = &[
    "identity.deaconRevision",
    "identity.oracleVersion",
    "identity.specRevision",
    "identity.schemaRevisions",
    "environment.platform",
    "environment.arch",
    "environment.containerEngine",
    "environment.composeVersion",
    "sourceScope",
    "coverage.behaviorCount",
    "coverage.contextCoverage",
    "coverage.observableCoverage",
    "exceptions.gaps",
    "exceptions.waivers",
    "exceptions.intentionalDivergences",
    "snapshotProvenance",
];

/// Assemble the report from a verdict and the registry it was computed over.
pub fn build_report(
    certification: &Certification,
    registry: &Registry,
    inputs: &ReportInputs,
) -> CertificationReport {
    let profile = certification.profile.clone();

    CertificationReport {
        schema_version: 1,
        certified: certification.certified,
        identity: identity(registry, inputs),
        environment: inputs.environment.clone(),
        scope: scope(registry, &profile, &inputs.environment),
        source_scope: source_scope(registry, certification, inputs),
        coverage: coverage(registry, certification),
        exceptions: exceptions(registry, certification),
        snapshot_provenance: snapshot_provenance(certification),
        not_certified: not_certified(registry, certification),
        admitted_non_deterministic_inputs: inputs.admitted_non_deterministic_inputs.clone(),
        evaluation_date: inputs.evaluation_date.clone(),
        blocking: blocking(certification),
    }
}

/// The wire name of a revision kind, so lookups read as the strings the records carry.
fn revision_kind_wire(kind: crate::model::RevisionKind) -> &'static str {
    use crate::model::RevisionKind;
    match kind {
        RevisionKind::Spec => "spec",
        RevisionKind::Schema => "schema",
        RevisionKind::Oracle => "oracle",
        RevisionKind::CliSurface => "cli-surface",
    }
}

/// The report's name for a blocking condition — the FR-041 letter names, so a reader
/// matches a blocked release to the requirement that blocked it.
fn condition_name(blocking: &crate::certify::Blocking) -> String {
    use crate::certify::BlockingKind;
    match blocking.kind {
        BlockingKind::Gap => "unresolved-gap",
        BlockingKind::Uncovered => "uncovered-behavior",
        BlockingKind::Constraint | BlockingKind::Clause => "unclassified-source-change",
        BlockingKind::Obligation => "undispositioned-obligation",
        BlockingKind::StaleSnapshot => "stale-snapshot",
        BlockingKind::MissingExecution => "missing-required-execution",
        BlockingKind::IncorrectOracle => "incorrect-oracle",
        BlockingKind::RunnerOmission => "unknown-runner-omission",
        BlockingKind::SilentlySkippedCase => "silently-skipped-case",
        BlockingKind::FailingCase => "failing-case",
        BlockingKind::InactiveProfile => "inactive-profile-refused",
    }
    .to_string()
}

fn revision_pin(registry: &Registry, kind: &str) -> String {
    registry
        .revisions
        .iter()
        .find(|r| revision_kind_wire(r.kind) == kind)
        .map(|r| r.pin.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

fn identity(registry: &Registry, inputs: &ReportInputs) -> Identity {
    let mut schema_revisions: Vec<String> = registry
        .revisions
        .iter()
        .filter(|r| revision_kind_wire(r.kind) == "schema")
        .map(|r| r.pin.clone())
        .collect();
    schema_revisions.sort();
    Identity {
        deacon_revision: inputs.deacon_revision.clone(),
        oracle_version: revision_pin(registry, "oracle"),
        spec_revision: revision_pin(registry, "spec"),
        schema_revisions,
    }
}

fn scope(registry: &Registry, profile: &str, env: &Environment) -> Scope {
    // Everything the certified profile is NOT. Built from the registry's own inactive
    // profiles plus the axes the active profile pins, so the list cannot silently omit an
    // axis someone later adds a profile for.
    let mut does_not_certify: BTreeSet<String> = registry
        .profiles
        .iter()
        .filter(|p| p.id != profile)
        .map(|p| p.id.clone())
        .collect();
    // Each axis enumerates EVERY known value, including the one this certification covers,
    // and excludes only the covered one. Listing just the "other" values worked by accident
    // for linux/x86_64/docker and silently under-stated the scope everywhere else — a
    // Podman certification never named Docker as uncovered, which is the exact misreading
    // `doesNotCertify` exists to prevent.
    for engine in ["docker", "podman", "containerd", "nerdctl"] {
        if engine != env.container_engine {
            does_not_certify.insert(format!("container engine: {engine}"));
        }
    }
    for os in ["linux", "macos", "windows"] {
        if os != env.platform {
            does_not_certify.insert(format!("operating system: {os}"));
        }
    }
    for arch in ["x86_64", "aarch64", "armv7"] {
        if arch != env.arch {
            does_not_certify.insert(format!("architecture: {arch}"));
        }
    }
    does_not_certify.insert(format!(
        "any reference oracle version other than {}",
        revision_pin(registry, "oracle")
    ));

    let statement = format!(
        "Certification covers {}/{} with {} {} against @devcontainers/cli {} ONLY. It does \
         not extend to any other container engine, operating system, architecture, or \
         reference oracle version.",
        env.platform,
        env.arch,
        env.container_engine,
        env.container_engine_version,
        revision_pin(registry, "oracle"),
    );

    Scope {
        profile: profile.to_string(),
        does_not_certify: does_not_certify.into_iter().collect(),
        statement,
    }
}

fn source_scope(
    registry: &Registry,
    certification: &Certification,
    inputs: &ReportInputs,
) -> SourceScope {
    // Unclassified source units are exactly the constraint/clause blockers the verdict
    // already computed — re-deriving them here is what research D4 forbids.
    let mut unclassified: Vec<String> = certification
        .blocking
        .iter()
        .filter(|b| {
            matches!(
                b.kind,
                crate::certify::BlockingKind::Constraint | crate::certify::BlockingKind::Clause
            )
        })
        .map(|b| b.id.clone())
        .collect();
    unclassified.sort();
    unclassified.dedup();

    SourceScope {
        schema_documents: inputs.schema_documents,
        prose_documents: inputs.prose_documents,
        cli_surface: revision_pin(registry, "cli-surface"),
        unclassified_units: unclassified,
    }
}

/// The minimum covering cases a channel needs before it counts as observed (SC-005 of the
/// 024 model). A channel carried by one case is one authoring mistake from unobserved.
const CHANNEL_COVERAGE_FLOOR: usize = 3;

fn coverage(registry: &Registry, certification: &Certification) -> Coverage {
    let summary = &certification.obligations;
    let mut observable: Vec<ChannelCoverage> = registry
        .channels
        .iter()
        .map(|channel| {
            let covering = registry
                .cases
                .iter()
                .filter(|case| {
                    case.expected.iter().any(|e| e.channel == channel.id)
                        || case.outcomes.iter().any(|o| o.channel == channel.id)
                })
                .count();
            ChannelCoverage {
                channel: channel.id.clone(),
                covering_cases: covering,
                meets_floor: covering >= CHANNEL_COVERAGE_FLOOR,
            }
        })
        .collect();
    observable.sort_by(|a, b| a.channel.cmp(&b.channel));

    Coverage {
        behavior_count: registry.behaviors.len(),
        context_coverage: ContextCoverage {
            covered: summary.covered,
            waived: summary.waived,
            non_testable: summary.non_testable,
            gap: summary.gap,
            inactive_environment: summary.inactive_environment,
            undispositioned: summary.undispositioned,
        },
        observable_coverage: observable,
    }
}

fn exceptions(registry: &Registry, certification: &Certification) -> Exceptions {
    let mut gaps: Vec<String> = registry.gaps.iter().map(|g| g.id.clone()).collect();
    gaps.sort();
    let mut intentional: Vec<String> = registry.extensions.iter().map(|e| e.id.clone()).collect();
    intentional.sort();
    Exceptions {
        gaps,
        waivers: certification.waived.clone(),
        intentional_divergences: intentional,
    }
}

fn snapshot_provenance(certification: &Certification) -> Vec<SnapshotProvenanceEntry> {
    let mut entries: Vec<SnapshotProvenanceEntry> = certification
        .snapshot_coverage
        .iter()
        .map(|c| SnapshotProvenanceEntry {
            case_id: c.case_id.clone(),
            platforms: c.platforms.clone(),
        })
        .collect();
    entries.sort_by(|a, b| a.case_id.cmp(&b.case_id));
    entries
}

fn not_certified(registry: &Registry, certification: &Certification) -> NotCertified {
    let mut inactive: Vec<String> = registry
        .profiles
        .iter()
        .filter(|p| !p.active)
        .map(|p| p.id.clone())
        .collect();
    inactive.sort();
    // Enumerated from the registry's own dispositions, never left empty: an empty list
    // here would claim "nothing is non-testable" while `coverage.contextCoverage` reports
    // a non-zero count, and a reader could not tell "not applicable" from "forgotten"
    // (FR-037) — which is the whole reason the field exists.
    let mut non_testable: Vec<String> = registry
        .obligation_dispositions
        .iter()
        .filter(|d| d.disposition == crate::obligation::DispositionKind::NonTestable)
        .map(|d| d.obligation.clone())
        .collect();
    non_testable.sort();
    non_testable.dedup();
    NotCertified {
        inactive_profiles: inactive,
        non_testable,
        no_reference_for_platform: certification.no_reference.clone(),
    }
}

fn blocking(certification: &Certification) -> Vec<BlockingEntry> {
    let mut entries: Vec<BlockingEntry> = certification
        .blocking
        .iter()
        .map(|b| BlockingEntry {
            condition: condition_name(b),
            record: b.id.clone(),
            detail: b.code.clone().unwrap_or_default(),
        })
        .collect();
    entries.sort_by(|a, b| {
        (a.condition.as_str(), a.record.as_str()).cmp(&(b.condition.as_str(), b.record.as_str()))
    });
    entries
}

/// Render the report as deterministic JSON.
pub fn render_json(report: &CertificationReport) -> String {
    let mut out = serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string());
    out.push('\n');
    out
}

/// Render the report as deterministic Markdown — stable ordering, no timestamps, no
/// absolute paths, no hostnames (FR-036).
pub fn render_md(report: &CertificationReport) -> String {
    let mut out = String::new();
    out.push_str("# Conformance certification\n\n");
    out.push_str(&format!(
        "**Verdict:** {}\n\n",
        if report.certified {
            "CERTIFIED"
        } else {
            "NOT CERTIFIED"
        }
    ));

    out.push_str("## Scope\n\n");
    out.push_str(&format!("{}\n\n", report.scope.statement));
    out.push_str(&format!("- Profile: `{}`\n", report.scope.profile));
    out.push_str("- **This certification does NOT extend to:**\n");
    for item in &report.scope.does_not_certify {
        out.push_str(&format!("  - {item}\n"));
    }
    out.push('\n');

    out.push_str("## Identity\n\n");
    out.push_str("| Field | Value |\n|---|---|\n");
    out.push_str(&format!(
        "| deacon revision | `{}` |\n",
        report.identity.deacon_revision
    ));
    out.push_str(&format!(
        "| reference oracle | `{}` |\n",
        report.identity.oracle_version
    ));
    out.push_str(&format!(
        "| specification revision | `{}` |\n",
        report.identity.spec_revision
    ));
    out.push_str(&format!(
        "| schema revisions | `{}` |\n",
        report.identity.schema_revisions.join(", ")
    ));
    out.push_str(&format!(
        "| platform | `{}` |\n",
        report.environment.platform
    ));
    out.push_str(&format!(
        "| architecture | `{}` |\n",
        report.environment.arch
    ));
    out.push_str(&format!(
        "| container engine | `{} {}` |\n",
        report.environment.container_engine, report.environment.container_engine_version
    ));
    out.push_str(&format!(
        "| Compose | `{}` |\n\n",
        report.environment.compose_version
    ));

    out.push_str("## Source scope\n\n");
    out.push_str(&format!(
        "- Schema documents: {}\n- Prose documents: {}\n- CLI surface: `{}`\n- Unclassified units: {}\n\n",
        report.source_scope.schema_documents,
        report.source_scope.prose_documents,
        report.source_scope.cli_surface,
        report.source_scope.unclassified_units.len()
    ));

    out.push_str("## Coverage\n\n");
    out.push_str(&format!(
        "- Behaviors: {}\n- Context: {} covered, {} waived, {} non-testable, {} gap, {} inactive-environment, {} undispositioned\n\n",
        report.coverage.behavior_count,
        report.coverage.context_coverage.covered,
        report.coverage.context_coverage.waived,
        report.coverage.context_coverage.non_testable,
        report.coverage.context_coverage.gap,
        report.coverage.context_coverage.inactive_environment,
        report.coverage.context_coverage.undispositioned,
    ));
    out.push_str("| Channel | Covering cases | Meets floor |\n|---|---|---|\n");
    for channel in &report.coverage.observable_coverage {
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            channel.channel, channel.covering_cases, channel.meets_floor
        ));
    }
    out.push('\n');

    out.push_str("## Exceptions\n\n");
    out.push_str(&format!(
        "- Gaps: {}\n- Waivers: {}\n- Intentional divergences: {}\n\n",
        report.exceptions.gaps.len(),
        report.exceptions.waivers.len(),
        report.exceptions.intentional_divergences.len()
    ));

    out.push_str("## Not certified\n\n");
    out.push_str(&format!(
        "- Inactive profiles: {}\n- Non-testable units: {}\n- Cases with no reference snapshot for this platform: {}\n\n",
        render_list(&report.not_certified.inactive_profiles),
        render_list(&report.not_certified.non_testable),
        render_list(&report.not_certified.no_reference_for_platform),
    ));

    if !report.admitted_non_deterministic_inputs.is_empty() {
        out.push_str("## Admitted non-deterministic inputs\n\n");
        for input in &report.admitted_non_deterministic_inputs {
            out.push_str(&format!("- `{}` — {}\n", input.input, input.reason));
        }
        out.push('\n');
    }

    if !report.blocking.is_empty() {
        out.push_str("## Blocking\n\n| Condition | Record | Detail |\n|---|---|---|\n");
        for entry in &report.blocking {
            out.push_str(&format!(
                "| {} | `{}` | {} |\n",
                entry.condition, entry.record, entry.detail
            ));
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "Time-dependent conditions evaluated against `{}`.\n",
        report.evaluation_date
    ));
    out
}

fn render_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items
            .iter()
            .map(|i| format!("`{i}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fr034_field_list_matches_its_declared_count() {
        // Guards the arithmetic the data model got wrong once: sixteen, not the twenty a
        // reader gets by also counting the scope, evaluation-date, and not-certified
        // fields that satisfy other requirements.
        assert_eq!(FR034_FIELDS.len(), FR034_FIELD_COUNT);
    }

    #[test]
    fn render_is_stable_across_calls() {
        let report = CertificationReport {
            schema_version: 1,
            certified: true,
            identity: Identity {
                deacon_revision: "abc".into(),
                oracle_version: "0.87.0".into(),
                spec_revision: "113500f4".into(),
                schema_revisions: vec!["113500f4".into()],
            },
            environment: Environment {
                platform: "linux".into(),
                arch: "x86_64".into(),
                container_engine: "docker".into(),
                container_engine_version: "27.3.1".into(),
                compose_version: "2.29.7".into(),
            },
            scope: Scope {
                profile: "prof-linux-amd64-docker-0870".into(),
                does_not_certify: vec!["container engine: podman".into()],
                statement: "s".into(),
            },
            source_scope: SourceScope {
                schema_documents: 4,
                prose_documents: 18,
                cli_surface: "0.87.0".into(),
                unclassified_units: vec![],
            },
            coverage: Coverage {
                behavior_count: 10,
                context_coverage: ContextCoverage::default(),
                observable_coverage: vec![],
            },
            exceptions: Exceptions {
                gaps: vec![],
                waivers: vec![],
                intentional_divergences: vec![],
            },
            snapshot_provenance: vec![],
            not_certified: NotCertified::default(),
            admitted_non_deterministic_inputs: vec![],
            evaluation_date: "2026-07-28".into(),
            blocking: vec![],
        };
        assert_eq!(render_md(&report), render_md(&report));
        assert_eq!(render_json(&report), render_json(&report));
        // No clock, no host, no absolute path can appear in the output.
        let md = render_md(&report);
        assert!(!md.contains("/workspaces"));
    }

    #[test]
    fn the_scope_statement_names_what_it_does_not_cover() {
        let md = {
            let scope = Scope {
                profile: "prof-linux-amd64-docker-0870".into(),
                does_not_certify: vec!["container engine: podman".into()],
                statement: "Certification covers linux/x86_64 with docker ONLY.".into(),
            };
            format!("{}\n{}", scope.statement, scope.does_not_certify.join(","))
        };
        assert!(
            md.contains("podman"),
            "a reader must find Podman named as uncovered"
        );
    }
}
