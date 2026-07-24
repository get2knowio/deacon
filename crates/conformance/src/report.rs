//! Deterministic coverage report generation (T017 `report.json`, T018 `report.md`).
//!
//! Both artifacts are derived purely from a validated [`Registry`] and its
//! [`Coverage`], with every collection ID-sorted before serialization and NO
//! timestamps, hostnames, or absolute paths anywhere (SC-004, research Decision 7).
//! `report.json` for identical registry content is therefore byte-identical across
//! runs and machines; `report.md` is rendered from the same ordered data.
//!
//! The full source → behavior → context → case → outcome chain (FR-022) is carried
//! on each behavior entry via `sources`, `applicability`, `cases` (with per-channel
//! `outcomes`), `waivers`, and `gaps`. The first seven `report.md` sections follow
//! contracts/report-schema.md exactly: header, summary, gaps (always present),
//! divergences & waivers, extensions, traceability index, out-of-profile behaviors.
//! An eighth "Constraint inventory" section (020-schema-constraint-inventory, T028)
//! summarizes the committed constraint inventory joined against the hand-authored
//! classification records — unit counts by document/kind, disposition tallies, and the
//! normally-empty unclassified/stale review queues.

use std::fmt::Write as _;
use std::path::Path;

use indexmap::IndexMap;
use serde::Serialize;

use crate::coverage::{BehaviorCoverage, Coverage, CoverageState};
use crate::load::Registry;
use crate::model::{
    Classification, ClauseClassification, ClauseInventory, Condition, ConstraintInventory,
    ConstraintKind, Decision, Disposition, ExpectedOutcome, GapKind, ReferenceStatus, RevisionKind,
    SpecStatus, Strength, Testability,
};
use crate::validate::join_inventory;

/// The `report.json` schema version (contracts/report-schema.md).
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// report.json — serde model (field order IS the on-disk order)
// ---------------------------------------------------------------------------

/// The complete `report.json` document (contracts/report-schema.md). Field order
/// here is the serialized key order; every nested collection is ID-sorted at build
/// time so the output is byte-stable for identical registry content (SC-004).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportJson {
    pub schema_version: u32,
    pub profile: Option<ProfileEntry>,
    pub revisions: Vec<RevisionEntry>,
    pub summary: Summary,
    pub behaviors: Vec<BehaviorEntry>,
    pub out_of_profile: Vec<OutOfProfileEntry>,
    pub extensions: Vec<ExtensionEntry>,
    pub gaps: Vec<GapEntry>,
    pub waivers: Vec<WaiverEntry>,
    /// Always empty in a valid registry (a source unit with neither behaviors nor
    /// `outOfScope` is V4); present for shape stability (contracts/report-schema.md).
    pub unclassified_source_units: Vec<String>,
    /// The schema-constraint inventory summary (020-schema-constraint-inventory,
    /// T028): committed-unit counts joined against the hand-authored classification
    /// records. Present-but-zeroed when the registry ships no sibling inventory
    /// (mirrors the validate V11–V14 scoping).
    pub inventory: InventorySection,
    /// The normative-clause inventory summary (021-normative-clause-inventory, T026):
    /// clause counts by strength/testability/document, disposition tallies, and the
    /// unclassified + ambiguous-pending review queues. Present-but-zeroed when the
    /// registry ships no sibling clause inventory.
    pub clauses: ClauseSection,
    /// Per-channel normalized-evidence coverage from declarative cases
    /// (022-conformance-runner US3, T048): for every declared observable channel, how
    /// many declarative cases exercise it and how many expectations carry an assertion
    /// (spec-expectation coverage). One entry per declared channel, channel-id-sorted, so
    /// the shape is stable (all channels present, zero when unused).
    pub channel_coverage: Vec<ChannelCoverageEntry>,
}

/// One channel's declarative-case coverage row (022-conformance-runner, T048).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelCoverageEntry {
    /// The `chan-…` id (declared in `channels.json`).
    pub channel: String,
    /// Number of declarative cases that declare an `expected` observable on this channel.
    pub declarative_cases: usize,
    /// Number of those expectations that carry an `assertion` (spec-expectation
    /// normalized-evidence coverage; live-differential/snapshot expectations may omit it).
    pub asserted_expectations: usize,
}

/// Build the per-channel declarative-evidence coverage (T048): one row per declared
/// channel, channel-id-sorted, counting declarative cases that exercise it and the
/// expectations carrying an assertion. Deterministic (all declared channels present).
fn build_channel_coverage(registry: &Registry) -> Vec<ChannelCoverageEntry> {
    let mut channels: Vec<&str> = registry.channels.iter().map(|c| c.id.as_str()).collect();
    channels.sort_unstable();
    channels
        .into_iter()
        .map(|channel| {
            let mut declarative_cases = 0usize;
            let mut asserted_expectations = 0usize;
            for case in &registry.cases {
                if !matches!(case.classify(), Ok(crate::model::CaseKind::Declarative)) {
                    continue;
                }
                let on_channel: Vec<_> = case
                    .expected
                    .iter()
                    .filter(|e| e.channel == channel)
                    .collect();
                if on_channel.is_empty() {
                    continue;
                }
                declarative_cases += 1;
                asserted_expectations +=
                    on_channel.iter().filter(|e| e.assertion.is_some()).count();
            }
            ChannelCoverageEntry {
                channel: channel.to_string(),
                declarative_cases,
                asserted_expectations,
            }
        })
        .collect()
}

/// The normative-clause-inventory section of `report.json`
/// (021-normative-clause-inventory, FR-017). Deterministic: strength/testability counts
/// in enum-declaration order, document counts document-key-sorted, listings ID-sorted.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClauseSection {
    /// The manifest revision the clause inventory was canonicalized from; empty when none.
    pub revision: String,
    /// Total clause units across all documents.
    pub total_units: usize,
    /// Clause counts keyed by document, document-key-sorted.
    pub units_by_document: IndexMap<String, usize>,
    /// Clause counts for every `Strength`, in declaration order (all six always present).
    pub units_by_strength: IndexMap<String, usize>,
    /// Clause counts for every `Testability`, in declaration order (all five present).
    pub units_by_testability: IndexMap<String, usize>,
    /// Clause-classification disposition tallies.
    pub dispositions: DispositionTally,
    /// Clause IDs with no effective disposition (the V12 review queue), ID-sorted.
    pub unclassified: Vec<String>,
    /// Ambiguous-testability clause IDs still awaiting a per-clause decision, ID-sorted.
    pub ambiguous_pending: Vec<String>,
    /// Clause-classification IDs whose clause is absent from the inventory (V11 stale),
    /// ID-sorted.
    pub stale: Vec<String>,
}

/// Every `Strength` in declaration order (deterministic emission).
const ALL_STRENGTHS: [Strength; 6] = [
    Strength::Must,
    Strength::Should,
    Strength::May,
    Strength::Algorithm,
    Strength::IoContract,
    Strength::Descriptive,
];

/// Every `Testability` in declaration order (deterministic emission).
const ALL_TESTABILITIES: [Testability; 5] = [
    Testability::DirectlyTestable,
    Testability::IndirectlyTestable,
    Testability::Informative,
    Testability::Ambiguous,
    Testability::NotApplicable,
];

/// The constraint-inventory section of `report.json` (020-schema-constraint-inventory,
/// FR-014). Summarizes the committed inventory (`conformance/inventory/constraints.json`)
/// joined against the hand-authored `cls-` classification records: unit counts by
/// document and by kind, disposition tallies, and the (normally empty) unclassified
/// and stale review queues. Every collection is deterministically ordered — document
/// counts by document key, kind counts in the closed `ConstraintKind` declaration
/// order (all 15 always present), listings ID-sorted — so it stays byte-stable (SC-004).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySection {
    /// The manifest revision the committed inventory was extracted from; empty string
    /// when no inventory is present.
    pub revision: String,
    /// Total extracted constraint units across all documents.
    pub total_units: usize,
    /// Unit counts keyed by manifest document, emitted in document-key-sorted order.
    pub units_by_document: IndexMap<String, usize>,
    /// Unit counts for every `ConstraintKind`, in the enum's declaration order — all
    /// 15 kinds always present (zero when unobserved), so the shape is stable.
    pub units_by_kind: IndexMap<String, usize>,
    /// Classification-disposition tallies across all classification records.
    pub dispositions: DispositionTally,
    /// Constraint unit IDs with zero classification records (the V12 review queue —
    /// normally empty), ID-sorted.
    pub unclassified: Vec<String>,
    /// Classification IDs whose `constraint` is absent from the committed inventory
    /// (the V11 stale queue — normally empty), ID-sorted.
    pub stale: Vec<String>,
}

/// Classification-disposition counts (`behavior-mapped` / `non-testable` /
/// `not-applicable`) — the consumer-only-scope boundary, kept visible per FR-014.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispositionTally {
    pub behavior_mapped: usize,
    pub non_testable: usize,
    pub not_applicable: usize,
}

/// Every `ConstraintKind` in declaration order — the deterministic emission order for
/// the inventory section's per-kind counts (all 15 always present).
const ALL_CONSTRAINT_KINDS: [ConstraintKind; 15] = [
    ConstraintKind::PropertyExistence,
    ConstraintKind::Required,
    ConstraintKind::Type,
    ConstraintKind::Enum,
    ConstraintKind::Const,
    ConstraintKind::Default,
    ConstraintKind::UnionAlternative,
    ConstraintKind::AllOf,
    ConstraintKind::Conditional,
    ConstraintKind::AdditionalProperties,
    ConstraintKind::ArrayShape,
    ConstraintKind::ValueShape,
    ConstraintKind::Reference,
    ConstraintKind::Annotation,
    ConstraintKind::UnmodeledKeyword,
];

/// The active profile's identity and its ID-sorted context assignment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileEntry {
    pub id: String,
    /// Dimension → value, emitted in dimension-ID-sorted order (determinism rule).
    pub context: IndexMap<String, String>,
}

/// A pinned source revision (`revisions[]`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RevisionEntry {
    pub id: String,
    pub kind: RevisionKind,
    pub pin: String,
}

/// The coverage summary — the ONLY denominator is `behaviors_in_profile` (FR-003);
/// `waived` is its own bucket, never folded into `conformant` (FR-023); `out_of_profile`
/// is excluded from the denominator (FR-017).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub behaviors_in_profile: usize,
    pub conformant: usize,
    pub divergent: usize,
    pub waived: usize,
    pub gap: usize,
    pub extensions: usize,
    pub out_of_profile: usize,
}

/// One in-profile, non-extension behavior with its coverage state and full trace.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BehaviorEntry {
    pub id: String,
    pub statement: String,
    /// `conformant` | `divergent` | `waived` | `gap`.
    pub coverage: String,
    pub spec: SpecStatus,
    pub reference: ReferenceStatus,
    pub decision: Decision,
    /// Source units referencing this behavior (trace: source → behavior).
    pub sources: Vec<String>,
    /// Applicability conditions (trace: behavior → context).
    pub applicability: Vec<Condition>,
    /// Covering cases with their expected outcomes (trace: → case → outcome).
    pub cases: Vec<CaseEntry>,
    pub waivers: Vec<String>,
    pub gaps: Vec<String>,
}

/// A covering test case with its per-channel expected outcomes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseEntry {
    pub id: String,
    pub outcomes: Vec<ExpectedOutcome>,
}

/// An out-of-profile behavior — listed with the conditions it awaits (FR-017).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutOfProfileEntry {
    pub id: String,
    pub applicability: Vec<Condition>,
}

/// A Deacon extension record and the behaviors it classifies (separate from
/// divergences, FR-012).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExtensionEntry {
    pub id: String,
    pub behaviors: Vec<String>,
}

/// A gap record (`gaps[]`) — always surfaced (FR-020).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GapEntry {
    pub id: String,
    pub kind: GapKind,
    pub behaviors: Vec<String>,
}

/// A waiver record (`waivers[]`) — rationale and expiry, non-blocking (FR-025).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WaiverEntry {
    pub id: String,
    pub rationale: String,
    pub expires: String,
}

// ---------------------------------------------------------------------------
// Building the report model
// ---------------------------------------------------------------------------

/// Build the `report.json` model from a validated registry (evaluates coverage
/// internally). All collections are ID-sorted (SC-004). `inventory` supplies the
/// committed constraint inventory for the inventory section — `None` when the
/// registry ships no sibling inventory (e.g. the V1–V10 acceptance fixtures), in
/// which case that section is present-but-zeroed.
pub fn build_report(registry: &Registry, inventory: Option<&ConstraintInventory>) -> ReportJson {
    build_report_full(
        registry,
        inventory,
        None,
        &Default::default(),
        &Default::default(),
    )
}

/// Build the report model, including the normative-clause-inventory section. `clauses` is
/// the committed clause inventory (`None` → present-but-zeroed); `authoring_docs` and
/// `covered_docs` drive the document-scope resolution of the clause join (mirroring
/// `validate::join_clauses`).
pub fn build_report_full(
    registry: &Registry,
    inventory: Option<&ConstraintInventory>,
    clauses: Option<&ClauseInventory>,
    authoring_docs: &std::collections::HashSet<String>,
    covered_docs: &std::collections::HashSet<String>,
) -> ReportJson {
    let coverage = Coverage::evaluate(registry);
    build_report_from_coverage(
        registry,
        &coverage,
        inventory,
        clauses,
        authoring_docs,
        covered_docs,
    )
}

/// Build the report model from a registry and its already-computed coverage.
fn build_report_from_coverage(
    registry: &Registry,
    coverage: &Coverage<'_>,
    inventory: Option<&ConstraintInventory>,
    clauses: Option<&ClauseInventory>,
    authoring_docs: &std::collections::HashSet<String>,
    covered_docs: &std::collections::HashSet<String>,
) -> ReportJson {
    let profile = coverage.profile.map(|p| {
        // Emit the context map in dimension-ID-sorted order (determinism rule).
        let mut pairs: Vec<(&String, &String)> = p.context.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        let mut context = IndexMap::new();
        for (dim, val) in pairs {
            context.insert(dim.clone(), val.clone());
        }
        ProfileEntry {
            id: p.id.clone(),
            context,
        }
    });

    let mut revisions: Vec<RevisionEntry> = registry
        .revisions
        .iter()
        .map(|r| RevisionEntry {
            id: r.id.clone(),
            kind: r.kind,
            pin: r.pin.clone(),
        })
        .collect();
    revisions.sort_by(|a, b| a.id.cmp(&b.id));

    let summary = Summary {
        behaviors_in_profile: coverage.behaviors_in_profile(),
        conformant: coverage.count_state(CoverageState::Conformant),
        divergent: coverage.count_state(CoverageState::Divergent),
        waived: coverage.count_state(CoverageState::Waived),
        gap: coverage.count_state(CoverageState::Gap),
        extensions: coverage.extensions_count(),
        out_of_profile: coverage.out_of_profile_count(),
    };

    // In-profile, non-extension behaviors (already ID-sorted in `coverage`).
    let behaviors: Vec<BehaviorEntry> = coverage
        .behaviors
        .iter()
        .filter(|b| !b.is_extension)
        .map(behavior_entry)
        .collect();

    let out_of_profile: Vec<OutOfProfileEntry> = coverage
        .out_of_profile
        .iter()
        .map(|b| OutOfProfileEntry {
            id: b.id.clone(),
            applicability: b.applicability.clone(),
        })
        .collect();

    let mut extensions: Vec<ExtensionEntry> = registry
        .extensions
        .iter()
        .map(|e| ExtensionEntry {
            id: e.id.clone(),
            behaviors: sorted(&e.behaviors),
        })
        .collect();
    extensions.sort_by(|a, b| a.id.cmp(&b.id));

    let mut gaps: Vec<GapEntry> = registry
        .gaps
        .iter()
        .map(|g| GapEntry {
            id: g.id.clone(),
            kind: g.kind,
            behaviors: sorted(&g.behaviors),
        })
        .collect();
    gaps.sort_by(|a, b| a.id.cmp(&b.id));

    let mut waivers: Vec<WaiverEntry> = registry
        .waivers
        .iter()
        .map(|w| WaiverEntry {
            id: w.id.clone(),
            rationale: w.rationale.clone(),
            expires: w.expires.clone(),
        })
        .collect();
    waivers.sort_by(|a, b| a.id.cmp(&b.id));

    let mut unclassified_source_units: Vec<String> = registry
        .sources
        .iter()
        .filter(|s| s.behaviors.is_empty() && s.out_of_scope.is_none())
        .map(|s| s.id.clone())
        .collect();
    unclassified_source_units.sort();

    let inventory_section = build_inventory_section(inventory, &registry.classifications);
    let clause_section = build_clause_section(
        clauses,
        &registry.clause_classifications,
        authoring_docs,
        covered_docs,
    );

    ReportJson {
        schema_version: SCHEMA_VERSION,
        profile,
        revisions,
        summary,
        behaviors,
        out_of_profile,
        extensions,
        gaps,
        waivers,
        unclassified_source_units,
        inventory: inventory_section,
        clauses: clause_section,
        channel_coverage: build_channel_coverage(registry),
    }
}

/// Summarize the committed clause inventory joined against the hand-authored
/// clause-classification records (021-normative-clause-inventory, T026). Deterministic;
/// present-but-zeroed when `clauses` is `None`.
fn build_clause_section(
    clauses: Option<&ClauseInventory>,
    classifications: &[ClauseClassification],
    authoring_docs: &std::collections::HashSet<String>,
    covered_docs: &std::collections::HashSet<String>,
) -> ClauseSection {
    use crate::validate::join_clauses;

    let mut units_by_strength: IndexMap<String, usize> = IndexMap::new();
    for s in ALL_STRENGTHS {
        units_by_strength.insert(enum_str(&s), 0);
    }
    let mut units_by_testability: IndexMap<String, usize> = IndexMap::new();
    for t in ALL_TESTABILITIES {
        units_by_testability.insert(enum_str(&t), 0);
    }

    let mut units_by_document: IndexMap<String, usize> = IndexMap::new();
    let mut revision = String::new();
    let mut total_units = 0usize;
    let mut ambiguous_pending: Vec<String> = Vec::new();

    // A clause counts as "pending" when ambiguous AND lacking a per-clause classification.
    let per_clause: std::collections::HashSet<&str> = classifications
        .iter()
        .filter_map(|c| c.clause.as_deref())
        .collect();

    if let Some(inv) = clauses {
        revision = inv.revision.clone();
        total_units = inv.units.len();
        for unit in &inv.units {
            *units_by_document.entry(unit.document.clone()).or_insert(0) += 1;
            *units_by_strength
                .entry(enum_str(&unit.strength))
                .or_insert(0) += 1;
            *units_by_testability
                .entry(enum_str(&unit.testability))
                .or_insert(0) += 1;
            if unit.testability == Testability::Ambiguous && !per_clause.contains(unit.id.as_str())
            {
                ambiguous_pending.push(unit.id.clone());
            }
        }
    }
    units_by_document.sort_keys();
    ambiguous_pending.sort();

    let mut dispositions = DispositionTally::default();
    for c in classifications {
        match c.disposition {
            Disposition::BehaviorMapped => dispositions.behavior_mapped += 1,
            Disposition::NonTestable => dispositions.non_testable += 1,
            Disposition::NotApplicable => dispositions.not_applicable += 1,
        }
    }

    let join = clauses
        .map(|inv| join_clauses(&inv.units, classifications, authoring_docs, covered_docs))
        .unwrap_or_default();

    ClauseSection {
        revision,
        total_units,
        units_by_document,
        units_by_strength,
        units_by_testability,
        dispositions,
        unclassified: join.unclassified,
        ambiguous_pending,
        stale: join.stale,
    }
}

/// Summarize the committed constraint inventory joined against the hand-authored
/// classification records (020-schema-constraint-inventory, T028). Deterministic:
/// document counts are document-key-sorted, kind counts follow the closed
/// `ConstraintKind` declaration order (all 15 present), and the unclassified/stale
/// listings are ID-sorted. When `inventory` is `None` the section is present-but-zeroed
/// (no inventory to join), and the stale queue is likewise empty — a classification
/// cannot be judged stale without an inventory to check it against (this mirrors the
/// validate V11–V14 scoping, not a silent fallback).
fn build_inventory_section(
    inventory: Option<&ConstraintInventory>,
    classifications: &[Classification],
) -> InventorySection {
    // Seed all 15 kinds at zero in declaration order so the shape is stable.
    let mut units_by_kind: IndexMap<String, usize> = IndexMap::new();
    for kind in ALL_CONSTRAINT_KINDS {
        units_by_kind.insert(enum_str(&kind), 0);
    }

    let mut units_by_document: IndexMap<String, usize> = IndexMap::new();
    let mut revision = String::new();
    let mut total_units = 0usize;

    if let Some(inv) = inventory {
        revision = inv.revision.clone();
        total_units = inv.units.len();
        for unit in &inv.units {
            *units_by_document.entry(unit.document.clone()).or_insert(0) += 1;
            *units_by_kind.entry(enum_str(&unit.kind)).or_insert(0) += 1;
        }
    }
    // Document counts in document-key order (determinism rule).
    units_by_document.sort_keys();

    // Disposition tallies over every classification record.
    let mut dispositions = DispositionTally::default();
    for c in classifications {
        match c.disposition {
            Disposition::BehaviorMapped => dispositions.behavior_mapped += 1,
            Disposition::NonTestable => dispositions.non_testable += 1,
            Disposition::NotApplicable => dispositions.not_applicable += 1,
        }
    }

    // The unclassified (V12) and stale (V11) review queues, computed by the SAME join
    // `validate` enforces with, so the report can never disagree with the gate. Both are
    // empty without an inventory: a classification cannot be judged stale with nothing to
    // check it against.
    let join = inventory
        .map(|inv| join_inventory(&inv.units, classifications))
        .unwrap_or_default();
    let (unclassified, stale) = (join.unclassified, join.stale);

    InventorySection {
        revision,
        total_units,
        units_by_document,
        units_by_kind,
        dispositions,
        unclassified,
        stale,
    }
}

/// Render one in-profile behavior's full traceability entry.
fn behavior_entry(bc: &BehaviorCoverage<'_>) -> BehaviorEntry {
    let cases = bc
        .cases
        .iter()
        .map(|c| CaseEntry {
            id: c.id.clone(),
            outcomes: c.outcomes.clone(),
        })
        .collect();
    BehaviorEntry {
        id: bc.behavior.id.clone(),
        statement: bc.behavior.statement.clone(),
        coverage: bc.state.as_str().to_string(),
        spec: bc.behavior.spec,
        reference: bc.behavior.reference,
        decision: bc.behavior.decision,
        sources: bc.sources.iter().map(|s| s.to_string()).collect(),
        applicability: bc.behavior.applicability.clone(),
        cases,
        waivers: bc.waivers.iter().map(|w| w.to_string()).collect(),
        gaps: bc.gaps.iter().map(|g| g.to_string()).collect(),
    }
}

/// Clone-and-sort a list of stable IDs.
fn sorted(ids: &[String]) -> Vec<String> {
    let mut out = ids.to_vec();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/// Serialize the `report.json` document — pretty-printed, newline-terminated, and
/// byte-stable for identical registry content (SC-004). The inventory section is
/// present-but-zeroed (no committed inventory joined); use [`write_reports`] to emit a
/// populated inventory section.
pub fn render_report_json(registry: &Registry) -> String {
    let report = build_report(registry, None);
    render_json(&report)
}

/// Pretty-print a built report to its canonical `report.json` string.
fn render_json(report: &ReportJson) -> String {
    // `to_string_pretty` is deterministic (2-space indent, declaration/field order);
    // serialization cannot fail for these plain owned structs.
    let mut out = serde_json::to_string_pretty(report)
        .unwrap_or_else(|e| unreachable!("report serialization is infallible: {e}"));
    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// report.md — human-readable rendering (seven sections, contract order)
// ---------------------------------------------------------------------------

/// Render `report.md` — the contract sections in order, derived from the same
/// ID-sorted model as `report.json` (FR-020, FR-022, FR-023). No timestamps. The
/// inventory section is present-but-zeroed; use [`write_reports`] for a populated one.
pub fn render_report_md(registry: &Registry) -> String {
    let report = build_report(registry, None);
    render_md(&report)
}

/// Render the markdown document from a built report model.
fn render_md(report: &ReportJson) -> String {
    let mut md = String::new();

    // 1. Header — profile identity and pinned source revisions (no timestamp).
    md.push_str("# Conformance Report\n\n");
    match &report.profile {
        Some(profile) => {
            let _ = writeln!(md, "**Profile:** `{}`\n", profile.id);
            md.push_str("| Dimension | Value |\n|-----------|-------|\n");
            for (dim, val) in &profile.context {
                let _ = writeln!(md, "| `{dim}` | `{val}` |");
            }
        }
        None => md.push_str("**Profile:** _none active_\n"),
    }
    md.push_str("\n**Source revisions:**\n\n");
    md.push_str("| Revision | Kind | Pin |\n|----------|------|-----|\n");
    for rev in &report.revisions {
        let _ = writeln!(
            md,
            "| `{}` | {} | `{}` |",
            rev.id,
            enum_str(&rev.kind),
            rev.pin
        );
    }

    // 2. Summary table — waived is its own row, never folded into conformant.
    md.push_str("\n## Summary\n\n");
    md.push_str("| Metric | Count |\n|--------|-------|\n");
    let s = &report.summary;
    let _ = writeln!(md, "| Behaviors in profile | {} |", s.behaviors_in_profile);
    let _ = writeln!(md, "| Conformant | {} |", s.conformant);
    let _ = writeln!(md, "| Divergent | {} |", s.divergent);
    let _ = writeln!(md, "| Waived | {} |", s.waived);
    let _ = writeln!(md, "| Gap | {} |", s.gap);
    let _ = writeln!(md, "| Extensions | {} |", s.extensions);
    let _ = writeln!(md, "| Out of profile | {} |", s.out_of_profile);

    // 3. Gaps — ALWAYS present; "None" when empty; gaps are never hidden (FR-020).
    md.push_str("\n## Gaps\n\n");
    if report.gaps.is_empty() {
        md.push_str("None.\n");
    } else {
        md.push_str("| Gap | Kind | Behaviors |\n|-----|------|-----------|\n");
        for gap in &report.gaps {
            let _ = writeln!(
                md,
                "| `{}` | {} | {} |",
                gap.id,
                enum_str(&gap.kind),
                code_list(&gap.behaviors)
            );
        }
    }

    // 4. Divergences & waivers — three-axis disposition, rationale, expiry.
    md.push_str("\n## Divergences & waivers\n\n");
    let divergent: Vec<&BehaviorEntry> = report
        .behaviors
        .iter()
        .filter(|b| b.coverage == "divergent" || b.coverage == "waived")
        .collect();
    if divergent.is_empty() {
        md.push_str("None.\n");
    } else {
        md.push_str(
            "| Behavior | Coverage | Spec | Reference | Decision | Waivers | Rationale / expiry |\n",
        );
        md.push_str(
            "|----------|----------|------|-----------|----------|---------|--------------------|\n",
        );
        for b in divergent {
            let waiver_detail = b
                .waivers
                .iter()
                .filter_map(|wid| report.waivers.iter().find(|w| &w.id == wid))
                .map(|w| format!("{}: {} (expires {})", w.id, w.rationale, w.expires))
                .collect::<Vec<_>>()
                .join("<br>");
            let _ = writeln!(
                md,
                "| `{}` | {} | {} | {} | {} | {} | {} |",
                b.id,
                b.coverage,
                enum_str(&b.spec),
                enum_str(&b.reference),
                enum_str(&b.decision),
                code_list(&b.waivers),
                if waiver_detail.is_empty() {
                    "—".to_string()
                } else {
                    waiver_detail
                },
            );
        }
    }

    // 5. Extensions — listed separately from divergences.
    md.push_str("\n## Extensions\n\n");
    if report.extensions.is_empty() {
        md.push_str("None.\n");
    } else {
        md.push_str("| Extension | Behaviors |\n|-----------|-----------|\n");
        for ext in &report.extensions {
            let _ = writeln!(md, "| `{}` | {} |", ext.id, code_list(&ext.behaviors));
        }
    }

    // 6. Behavior traceability index — sources, contexts, cases, outcomes.
    md.push_str("\n## Behavior traceability index\n\n");
    if report.behaviors.is_empty() {
        md.push_str("No in-profile behaviors.\n\n");
    } else {
        for b in &report.behaviors {
            let _ = writeln!(md, "### `{}`\n", b.id);
            let _ = writeln!(md, "- **Statement:** {}", b.statement);
            let _ = writeln!(md, "- **Coverage:** {}", b.coverage);
            let _ = writeln!(
                md,
                "- **Disposition:** spec `{}`, reference `{}`, decision `{}`",
                enum_str(&b.spec),
                enum_str(&b.reference),
                enum_str(&b.decision)
            );
            let _ = writeln!(md, "- **Sources:** {}", code_list(&b.sources));
            let _ = writeln!(md, "- **Contexts:** {}", conditions_str(&b.applicability));
            if b.cases.is_empty() {
                md.push_str("- **Cases:** none\n");
            } else {
                md.push_str("- **Cases:**\n");
                for case in &b.cases {
                    let _ = writeln!(md, "  - `{}`", case.id);
                    for outcome in &case.outcomes {
                        let _ =
                            writeln!(md, "    - `{}`: {}", outcome.channel, outcome.expectation);
                    }
                }
            }
            if !b.waivers.is_empty() {
                let _ = writeln!(md, "- **Waivers:** {}", code_list(&b.waivers));
            }
            if !b.gaps.is_empty() {
                let _ = writeln!(md, "- **Gaps:** {}", code_list(&b.gaps));
            }
            md.push('\n');
        }
    }

    // 7. Out-of-profile behaviors — with the conditions they await.
    md.push_str("## Out-of-profile behaviors\n\n");
    if report.out_of_profile.is_empty() {
        md.push_str("None.\n");
    } else {
        md.push_str("| Behavior | Awaiting conditions |\n|----------|---------------------|\n");
        for b in &report.out_of_profile {
            let _ = writeln!(md, "| `{}` | {} |", b.id, conditions_str(&b.applicability));
        }
    }

    // 8. Constraint inventory — committed units joined against classifications.
    render_inventory_md(&mut md, &report.inventory);

    // 9. Normative clause inventory — committed clauses joined against classifications.
    render_clause_md(&mut md, &report.clauses);

    // 10. Declarative channel coverage — normalized-evidence coverage per channel.
    render_channel_coverage_md(&mut md, &report.channel_coverage);

    md
}

/// Render the per-channel declarative-evidence coverage section of `report.md`
/// (022-conformance-runner, T048). Deterministic (channel-id-sorted); "none" when no
/// declarative case exercises any channel yet.
fn render_channel_coverage_md(md: &mut String, coverage: &[ChannelCoverageEntry]) {
    md.push_str("\n## Declarative channel coverage\n\n");
    let exercised: Vec<&ChannelCoverageEntry> = coverage
        .iter()
        .filter(|c| c.declarative_cases > 0)
        .collect();
    if exercised.is_empty() {
        md.push_str("No declarative case exercises any channel yet.\n");
        return;
    }
    md.push_str("| Channel | Declarative cases | Asserted expectations |\n");
    md.push_str("|---------|-------------------|-----------------------|\n");
    for entry in exercised {
        let _ = writeln!(
            md,
            "| `{}` | {} | {} |",
            entry.channel, entry.declarative_cases, entry.asserted_expectations
        );
    }
}

/// Render the normative-clause-inventory section of `report.md`
/// (021-normative-clause-inventory, T026). Deterministic; empty-inventory renders "none".
fn render_clause_md(md: &mut String, section: &ClauseSection) {
    md.push_str("\n## Normative clause inventory\n\n");
    if section.total_units == 0 {
        md.push_str("No committed clause inventory.\n");
        return;
    }
    let _ = writeln!(md, "**Revision:** `{}`\n", section.revision);
    let _ = writeln!(md, "**Total clauses:** {}\n", section.total_units);

    md.push_str("### Clauses by document\n\n");
    md.push_str("| Document | Clauses |\n|----------|---------|\n");
    for (doc, count) in &section.units_by_document {
        let _ = writeln!(md, "| `{doc}` | {count} |");
    }

    md.push_str("\n### Clauses by strength\n\n");
    md.push_str("| Strength | Clauses |\n|----------|---------|\n");
    for (strength, count) in &section.units_by_strength {
        let _ = writeln!(md, "| `{strength}` | {count} |");
    }

    md.push_str("\n### Clauses by testability\n\n");
    md.push_str("| Testability | Clauses |\n|-------------|---------|\n");
    for (testability, count) in &section.units_by_testability {
        let _ = writeln!(md, "| `{testability}` | {count} |");
    }

    md.push_str("\n### Dispositions\n\n");
    md.push_str("| Disposition | Count |\n|-------------|-------|\n");
    let _ = writeln!(
        md,
        "| `behavior-mapped` | {} |",
        section.dispositions.behavior_mapped
    );
    let _ = writeln!(
        md,
        "| `non-testable` | {} |",
        section.dispositions.non_testable
    );
    let _ = writeln!(
        md,
        "| `not-applicable` | {} |",
        section.dispositions.not_applicable
    );

    md.push_str("\n### Unclassified clauses\n\n");
    if section.unclassified.is_empty() {
        md.push_str("None.\n");
    } else {
        for id in &section.unclassified {
            let _ = writeln!(md, "- `{id}`");
        }
    }

    md.push_str("\n### Ambiguous pending review\n\n");
    if section.ambiguous_pending.is_empty() {
        md.push_str("None.\n");
    } else {
        for id in &section.ambiguous_pending {
            let _ = writeln!(md, "- `{id}`");
        }
    }

    md.push_str("\n### Stale clause classifications\n\n");
    if section.stale.is_empty() {
        md.push_str("None.\n");
    } else {
        for id in &section.stale {
            let _ = writeln!(md, "- `{id}`");
        }
    }
}

/// Render the constraint-inventory section of `report.md`
/// (020-schema-constraint-inventory, T028). Derived from the same deterministic model
/// as the JSON section; empty-inventory registries render an explicit "none" state.
fn render_inventory_md(md: &mut String, inv: &InventorySection) {
    md.push_str("\n## Constraint inventory\n\n");
    if inv.total_units == 0 {
        md.push_str("No committed inventory.\n");
        return;
    }
    let _ = writeln!(md, "**Revision:** `{}`\n", inv.revision);
    let _ = writeln!(md, "**Total units:** {}\n", inv.total_units);

    md.push_str("### Units by document\n\n");
    md.push_str("| Document | Units |\n|----------|-------|\n");
    for (doc, count) in &inv.units_by_document {
        let _ = writeln!(md, "| `{doc}` | {count} |");
    }

    md.push_str("\n### Units by kind\n\n");
    md.push_str("| Kind | Units |\n|------|-------|\n");
    for (kind, count) in &inv.units_by_kind {
        let _ = writeln!(md, "| `{kind}` | {count} |");
    }

    md.push_str("\n### Dispositions\n\n");
    md.push_str("| Disposition | Count |\n|-------------|-------|\n");
    let _ = writeln!(
        md,
        "| `behavior-mapped` | {} |",
        inv.dispositions.behavior_mapped
    );
    let _ = writeln!(md, "| `non-testable` | {} |", inv.dispositions.non_testable);
    let _ = writeln!(
        md,
        "| `not-applicable` | {} |",
        inv.dispositions.not_applicable
    );

    md.push_str("\n### Unclassified units\n\n");
    if inv.unclassified.is_empty() {
        md.push_str("None.\n");
    } else {
        for id in &inv.unclassified {
            let _ = writeln!(md, "- `{id}`");
        }
    }

    md.push_str("\n### Stale classifications\n\n");
    if inv.stale.is_empty() {
        md.push_str("None.\n");
    } else {
        for id in &inv.stale {
            let _ = writeln!(md, "- `{id}`");
        }
    }
}

/// Serialize a closed enum to its wire spelling (via serde_json), stripping the JSON
/// quotes — used for the human-readable markdown cells.
fn enum_str<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|e| unreachable!("enum serialization is infallible: {e}"))
        .trim_matches('"')
        .to_string()
}

/// Render a list of stable IDs as inline-code, comma-separated, or an em dash when
/// empty.
fn code_list(ids: &[String]) -> String {
    if ids.is_empty() {
        return "—".to_string();
    }
    ids.iter()
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render a set of applicability conditions for markdown; "any" when unconstrained.
fn conditions_str(conditions: &[Condition]) -> String {
    if conditions.is_empty() {
        return "any".to_string();
    }
    conditions
        .iter()
        .map(|c| format!("`{}` ∈ {{{}}}", c.dimension, c.values.join(", ")))
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------------
// Writing artifacts
// ---------------------------------------------------------------------------

/// Write `report.json` and `report.md` into `out_dir` (created if absent). Returns
/// the two written paths. IO failures propagate as `std::io::Error` (the CLI maps
/// them to exit code 2). `inventory` supplies the committed constraint inventory for
/// the inventory section (`None` → present-but-zeroed).
pub fn write_reports(
    registry: &Registry,
    inventory: Option<&ConstraintInventory>,
    clauses: Option<&ClauseInventory>,
    authoring_docs: &std::collections::HashSet<String>,
    covered_docs: &std::collections::HashSet<String>,
    out_dir: &Path,
) -> std::io::Result<(std::path::PathBuf, std::path::PathBuf)> {
    std::fs::create_dir_all(out_dir)?;
    let report = build_report_full(registry, inventory, clauses, authoring_docs, covered_docs);

    let json_path = out_dir.join("report.json");
    std::fs::write(&json_path, render_json(&report))?;

    let md_path = out_dir.join("report.md");
    std::fs::write(&md_path, render_md(&report))?;

    Ok((json_path, md_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_registry() -> Registry {
        let root = crate::workspace_root().join("fixtures/conformance/valid");
        Registry::load(&root).expect("valid fixture loads")
    }

    #[test]
    fn report_json_is_byte_identical_across_runs() {
        let registry = valid_registry();
        let a = render_report_json(&registry);
        let b = render_report_json(&registry);
        assert_eq!(
            a, b,
            "report.json must be byte-identical for identical input"
        );
        assert!(a.ends_with('\n'), "report.json must be newline-terminated");
    }

    #[test]
    fn report_json_has_no_absolute_paths_or_timestamps() {
        let registry = valid_registry();
        let json = render_report_json(&registry);
        assert!(
            !json.contains("/workspaces/"),
            "report.json must not leak absolute paths: {json}"
        );
        // No ISO timestamp with a time component (determinism, SC-004).
        assert!(
            !json.contains("T00:") && !json.contains("Z\""),
            "report.json must not embed timestamps"
        );
    }

    #[test]
    fn summary_counts_match_the_valid_fixture() {
        let registry = valid_registry();
        let report = build_report(&registry, None);
        let s = &report.summary;
        assert_eq!(s.behaviors_in_profile, 4);
        assert_eq!(s.conformant, 1);
        assert_eq!(s.divergent, 0);
        assert_eq!(s.waived, 1);
        assert_eq!(s.gap, 1);
        assert_eq!(s.extensions, 1);
        assert_eq!(s.out_of_profile, 1);
        // Five buckets sum to the denominator.
        assert_eq!(
            s.conformant + s.divergent + s.waived + s.gap + s.extensions,
            s.behaviors_in_profile
        );
    }

    #[test]
    fn report_md_has_the_eight_sections_in_order() {
        let registry = valid_registry();
        let md = render_report_md(&registry);
        let sections = [
            "# Conformance Report",
            "## Summary",
            "## Gaps",
            "## Divergences & waivers",
            "## Extensions",
            "## Behavior traceability index",
            "## Out-of-profile behaviors",
            "## Constraint inventory",
        ];
        let mut last = 0usize;
        for section in sections {
            let at = md
                .find(section)
                .unwrap_or_else(|| panic!("report.md missing section {section:?}\n{md}"));
            assert!(at >= last, "section {section:?} out of order in report.md");
            last = at;
        }
    }

    /// A synthetic inventory + classifications exercising the populated section:
    /// counts by document/kind, disposition tallies, and the unclassified/stale queues.
    fn synthetic_inventory() -> ConstraintInventory {
        use crate::model::{ConstraintKind, ConstraintUnit};
        let unit = |id: &str, document: &str, kind: ConstraintKind| ConstraintUnit {
            id: id.to_string(),
            document: document.to_string(),
            pointer: "/x".to_string(),
            kind,
            substance: serde_json::json!({}),
            context: None,
        };
        ConstraintInventory {
            schema_version: 1,
            revision: "rev-schema-113500f4".to_string(),
            units: vec![
                unit("cst-base-a-type-00000000", "base", ConstraintKind::Type),
                unit("cst-base-b-type-11111111", "base", ConstraintKind::Type),
                unit(
                    "cst-feature-c-enum-22222222",
                    "feature",
                    ConstraintKind::Enum,
                ),
            ],
        }
    }

    #[test]
    fn inventory_section_is_zeroed_when_no_inventory() {
        let section = build_inventory_section(None, &[]);
        assert_eq!(section.revision, "");
        assert_eq!(section.total_units, 0);
        assert!(section.units_by_document.is_empty());
        // All 15 kinds are present, every count zero (stable shape).
        assert_eq!(section.units_by_kind.len(), 15);
        assert!(section.units_by_kind.values().all(|&c| c == 0));
        assert_eq!(section.dispositions, DispositionTally::default());
        assert!(section.unclassified.is_empty());
        assert!(section.stale.is_empty());
    }

    #[test]
    fn inventory_section_counts_and_join_are_correct() {
        let inv = synthetic_inventory();
        // Classify two of three units; one classification is stale (unknown constraint).
        let classifications = vec![
            Classification {
                id: "cls-base-a-type-00000000".to_string(),
                constraint: "cst-base-a-type-00000000".to_string(),
                disposition: Disposition::BehaviorMapped,
                behaviors: vec!["bhv-x".to_string()],
                rationale: None,
                notes: None,
            },
            Classification {
                id: "cls-feature-c-enum-22222222".to_string(),
                constraint: "cst-feature-c-enum-22222222".to_string(),
                disposition: Disposition::NotApplicable,
                behaviors: vec![],
                rationale: Some("editor-only".to_string()),
                notes: None,
            },
            Classification {
                id: "cls-gone-type-99999999".to_string(),
                constraint: "cst-gone-type-99999999".to_string(),
                disposition: Disposition::NonTestable,
                behaviors: vec![],
                rationale: Some("removed upstream".to_string()),
                notes: None,
            },
        ];
        let section = build_inventory_section(Some(&inv), &classifications);

        assert_eq!(section.revision, "rev-schema-113500f4");
        assert_eq!(section.total_units, 3);
        assert_eq!(section.units_by_document.get("base"), Some(&2));
        assert_eq!(section.units_by_document.get("feature"), Some(&1));
        // Document keys are sorted.
        let docs: Vec<&String> = section.units_by_document.keys().collect();
        assert_eq!(docs, vec!["base", "feature"]);

        assert_eq!(section.units_by_kind.get("type"), Some(&2));
        assert_eq!(section.units_by_kind.get("enum"), Some(&1));
        assert_eq!(section.units_by_kind.get("required"), Some(&0));
        // Declaration order preserved (type precedes enum).
        let kinds: Vec<&String> = section.units_by_kind.keys().collect();
        assert_eq!(
            kinds.first().map(|s| s.as_str()),
            Some("property-existence")
        );

        assert_eq!(section.dispositions.behavior_mapped, 1);
        assert_eq!(section.dispositions.not_applicable, 1);
        assert_eq!(section.dispositions.non_testable, 1);

        // The unclassified unit (b) and the stale classification (gone) are surfaced.
        assert_eq!(section.unclassified, vec!["cst-base-b-type-11111111"]);
        assert_eq!(section.stale, vec!["cls-gone-type-99999999"]);
    }

    #[test]
    fn inventory_section_renders_deterministic_markdown() {
        let inv = synthetic_inventory();
        let mut a = String::new();
        render_inventory_md(&mut a, &build_inventory_section(Some(&inv), &[]));
        let mut b = String::new();
        render_inventory_md(&mut b, &build_inventory_section(Some(&inv), &[]));
        assert_eq!(a, b, "inventory markdown must be byte-identical");
        assert!(a.contains("## Constraint inventory"));
        assert!(a.contains("| `base` | 2 |"));
        assert!(a.contains("| `feature` | 1 |"));
        assert!(a.contains("| `type` | 2 |"));
    }
}
