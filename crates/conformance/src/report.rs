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
//! `outcomes`), `waivers`, and `gaps`. The seven-section `report.md` layout follows
//! contracts/report-schema.md exactly: header, summary, gaps (always present),
//! divergences & waivers, extensions, traceability index, out-of-profile behaviors.

use std::fmt::Write as _;
use std::path::Path;

use indexmap::IndexMap;
use serde::Serialize;

use crate::coverage::{BehaviorCoverage, Coverage, CoverageState};
use crate::load::Registry;
use crate::model::{
    Condition, Decision, ExpectedOutcome, GapKind, ReferenceStatus, RevisionKind, SpecStatus,
};

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
}

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
/// internally). All collections are ID-sorted (SC-004).
pub fn build_report(registry: &Registry) -> ReportJson {
    let coverage = Coverage::evaluate(registry);
    build_report_from_coverage(registry, &coverage)
}

/// Build the report model from a registry and its already-computed coverage.
fn build_report_from_coverage(registry: &Registry, coverage: &Coverage<'_>) -> ReportJson {
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
/// byte-stable for identical registry content (SC-004).
pub fn render_report_json(registry: &Registry) -> String {
    let report = build_report(registry);
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

/// Render `report.md` — the seven contract sections in order, derived from the same
/// ID-sorted model as `report.json` (FR-020, FR-022, FR-023). No timestamps.
pub fn render_report_md(registry: &Registry) -> String {
    let report = build_report(registry);
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

    md
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
/// them to exit code 2).
pub fn write_reports(
    registry: &Registry,
    out_dir: &Path,
) -> std::io::Result<(std::path::PathBuf, std::path::PathBuf)> {
    std::fs::create_dir_all(out_dir)?;
    let report = build_report(registry);

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
        let report = build_report(&registry);
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
    fn report_md_has_the_seven_sections_in_order() {
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
}
