//! Generate the human-facing parity page (`docs/PARITY.md`) from the registry.
//!
//! The page this replaces was hand-maintained, and it drifted twice within an hour of
//! being published — once because the counts were taken from a working tree that had
//! unmerged records in it, once because a PR merged behind it. A page whose stated
//! premise is "every claim traces to a committed record" cannot be typed.
//!
//! ## What the page is, and what it deliberately is not
//!
//! It answers ONE question per cell, for a reader who wants to know whether deacon will
//! behave like the reference CLI for their configuration. It does NOT expose the model
//! we use to maintain conformance: the three-axis disposition, evidence tiers, waivers,
//! residuals and obligations all stay in `conformance/RULES.md` and the generated
//! coverage report. Publishing the maintenance vocabulary as if it were the status is
//! what made the first version unreadable.
//!
//! ## The grid
//!
//! Rows are behaviors. Columns are the **cross product** of the scenario dimensions that
//! apply to the area — for `up`, config-source × features, so fifteen columns. An earlier
//! draft used the two dimensions as separate PROJECTIONS (three columns plus five), which
//! is smaller and actively misleading: a behavior covered only as image×single and
//! dockerfile×none projects to "img ✅ dkr ✅ / none ✅ 1 ✅" and reads as broad coverage.
//! The cross product shows two cells of fifteen. The projection flatters; the product
//! does not, so the product is what ships.
//!
//! Each cell carries the status **in that scenario**, from one vocabulary — an earlier
//! draft used ✅ to mean "same as reference" in a status column and "checked against the
//! reference" in the cells, so a deliberately-divergent row rendered as a line of green.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::load::Registry;
use crate::model::{Decision, OracleType, ReferenceStatus, RevisionKind, TestCase};
use crate::scenario::ScenarioModel;

/// A cell's verdict: exactly one meaning per glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    /// Same as the reference, and verified against it in this scenario.
    SameVerified,
    /// Believed the same, but the only evidence here is deacon-side.
    SameUnverified,
    /// A deliberate difference, characterized.
    DiffersOnPurpose,
    /// A difference we intend to remove.
    DiffersFixing,
    /// deacon-only capability; the reference has no equivalent to compare against.
    DeaconOnly,
    /// No case exercises this scenario.
    Unchecked,
    /// The behavior does not arise in this scenario — not a gap. Rendered blank rather
    /// than with a glyph: there is no question here to answer, so any mark would invite
    /// one.
    NotApplicable,
}

impl Cell {
    fn glyph(self) -> &'static str {
        match self {
            Cell::SameVerified => "✅",
            Cell::SameUnverified => "◐",
            Cell::DiffersOnPurpose => "⚠️",
            Cell::DiffersFixing => "❌",
            Cell::DeaconOnly => "🔵",
            Cell::Unchecked => "·",
            Cell::NotApplicable => "",
        }
    }
}

/// The per-scenario status a behavior takes wherever it IS covered, derived from its
/// three-axis disposition. The axes are collapsed here and nowhere else.
fn covered_verdict(b: &crate::model::BehaviorUnit, live: bool) -> Cell {
    match (b.decision, b.reference) {
        (Decision::DeaconExtension, _) | (_, ReferenceStatus::NotApplicable) => Cell::DeaconOnly,
        (Decision::IntentionalDivergence, _) => Cell::DiffersOnPurpose,
        // Divergent while intending to follow the spec or the reference means we are
        // behind and mean to fix it — the only honest reading of R5/R6.
        (_, ReferenceStatus::Divergent) => Cell::DiffersFixing,
        _ if live => Cell::SameVerified,
        _ => Cell::SameUnverified,
    }
}

/// A scenario dimension rendered as a column group.
///
/// Which dimensions appear for an area comes from the applicability rules, not from
/// this list: a dimension the rules pin to one value for an operation contributes no
/// information and is dropped, so `doctor` carries no Feature columns.
struct Dim {
    /// The `sdim-` id in the scenario model.
    key: &'static str,
    /// `(scenario value, column heading)`, in the order they read best. The headings
    /// are the group header; the dimension's own name is not rendered — a third
    /// header row reading "config"/"features" adds a line and no information.
    values: &'static [(&'static str, &'static str)],
}

/// A dimension narrowed to the values its operations actually permit.
struct ActiveDim<'a> {
    key: &'a str,
    values: Vec<(&'a str, &'a str)>,
}

const DIMS: &[Dim] = &[
    Dim {
        key: "sdim-config-source",
        values: &[("image", "img"), ("dockerfile", "dkr"), ("compose", "cmp")],
    },
    Dim {
        key: "sdim-features",
        values: &[
            ("none", "–"),
            ("single", "1"),
            ("multiple-declared-order", "many"),
            ("multiple-dependency-order", "deps"),
            ("lockfile", "lock"),
        ],
    },
];

fn area_title(area: &str) -> String {
    area.replace('-', " ")
}

/// Render the page. Deterministic: same registry in, same bytes out.
pub fn render(registry: &Registry) -> String {
    // behavior -> set of (scenario key tuple, live?) drawn from its covering cases.
    let mut coverage: BTreeMap<&str, Vec<(&TestCase, bool)>> = BTreeMap::new();
    for case in &registry.cases {
        // A case whose every declared assertion is vacuous checks nothing, so it must
        // not colour a cell. `·` already means "not checked in this scenario", and a
        // check that cannot fail IS not checking it — this is a correction to an
        // existing cell, not a new tier. V37 makes such a case unauthorable; this keeps
        // the page's claim true by construction rather than by that gate holding.
        if !case.expected.is_empty()
            && case.expected.iter().all(|e| {
                e.assertion
                    .as_ref()
                    .is_some_and(crate::model::is_vacuous_assertion)
            })
        {
            continue;
        }
        let live = matches!(case.oracle_type, Some(OracleType::LiveDifferential));
        for b in &case.behaviors {
            coverage.entry(b.as_str()).or_default().push((case, live));
        }
    }

    let mut areas: BTreeMap<&str, Vec<&crate::model::BehaviorUnit>> = BTreeMap::new();
    for b in &registry.behaviors {
        areas.entry(b.area.as_str()).or_default().push(b);
    }

    let model = ScenarioModel::new(&registry.scenario, &registry.applicability);

    let waivers: BTreeMap<&str, &str> = registry
        .waivers
        .iter()
        .flat_map(|w| w.behaviors.iter().map(|b| (b.as_str(), w.id.as_str())))
        .collect();

    let mut out = String::new();
    out.push_str(&header(registry));

    for (area, behaviors) in &areas {
        // Which dimensions can this area's operation actually VARY? Taken from the
        // applicability rules, not from what the cases happen to use: a dimension the
        // rules pin to a single value (Features for `doctor`, which resolves nothing)
        // would otherwise render five columns that can never be anything but `·`, and
        // an area with thin coverage would lose the columns that show it is thin.
        let operations: BTreeSet<&str> = behaviors
            .iter()
            .filter_map(|b| coverage.get(b.id.as_str()))
            .flatten()
            .filter_map(|(case, _)| case.scenario_context.get("sdim-operation"))
            .map(String::as_str)
            .collect();
        let mut permitted: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for op in &operations {
            for (dim, values) in model.applicable_dimensions(op) {
                permitted
                    .entry(dim.id.as_str())
                    .or_default()
                    .extend(values.iter().copied());
            }
        }
        // Render only the values the applicability rules PERMIT for this area's
        // operations. Filtering the dimension but not its values rendered columns that
        // can never be anything but `·` — `outdated` never resolves a dependency graph,
        // so a `deps` column there reads as an untested hole where the truth is that the
        // scenario cannot exist. That is the inverse of the projection problem: it makes
        // coverage look WORSE than it is, and is just as dishonest.
        let active: Vec<ActiveDim<'_>> = DIMS
            .iter()
            .filter_map(|d| {
                let allowed = permitted.get(d.key)?;
                let values: Vec<(&str, &str)> = d
                    .values
                    .iter()
                    .filter(|(value, _)| allowed.contains(value))
                    .copied()
                    .collect();
                (values.len() > 1).then_some(ActiveDim { key: d.key, values })
            })
            .collect();

        let _ = writeln!(out, "\n### {}\n", area_title(area));

        if active.is_empty() {
            // No scenario evidence to grid: a flat list is honest and smaller.
            out.push_str("| | Behavior | Notes |\n|---|---|---|\n");
            for b in behaviors {
                // No covering case at all is `·`, not a verdict. Falling through to
                // `covered_verdict` here rendered ◐ — "believed the same" — for a
                // behavior nothing has ever exercised, which is a claim, not a gap.
                let cell = match coverage.get(b.id.as_str()) {
                    None => Cell::Unchecked,
                    Some(cs) => covered_verdict(b, cs.iter().any(|(_, l)| *l)),
                };
                let _ = writeln!(
                    out,
                    "| {} | {} | {} |",
                    cell.glyph(),
                    first_sentence(&b.statement),
                    notes_cell(b, &waivers)
                );
            }
            continue;
        }

        out.push_str(&grid(&active, behaviors, &coverage, &waivers));
    }

    out.push_str(&footer());
    out
}

fn grid(
    active: &[ActiveDim<'_>],
    behaviors: &[&crate::model::BehaviorUnit],
    coverage: &BTreeMap<&str, Vec<(&TestCase, bool)>>,
    waivers: &BTreeMap<&str, &str>,
) -> String {
    // Cross product of the active dimensions' values, in declared order.
    let mut combos: Vec<Vec<(&str, &str)>> = vec![Vec::new()];
    for dim in active {
        let mut next = Vec::new();
        for base in &combos {
            for (value, _) in dim.values.iter() {
                let mut c = base.clone();
                c.push((dim.key, value));
                next.push(c);
            }
        }
        combos = next;
    }

    let mut s = String::new();
    s.push_str("<table>\n<thead>\n");
    if active.len() > 1 {
        // Two header rows: the outer dimension spans the inner one's width.
        let inner: usize = active[1..].iter().map(|d| d.values.len()).product();
        s.push_str("<tr><th rowspan=\"2\"></th><th rowspan=\"2\">Behavior</th>");
        for (_, label) in active[0].values.iter() {
            let _ = write!(s, "<th colspan=\"{inner}\">{label}</th>");
        }
        s.push_str("<th rowspan=\"2\">Notes</th></tr>\n<tr>");
        for _ in active[0].values.iter() {
            for (_, short) in active[1].values.iter() {
                let _ = write!(s, "<th>{short}</th>");
            }
        }
        s.push_str("</tr>\n");
    } else {
        // One dimension: a single header row. Emitting the two-row form anyway left an
        // empty second row of `<th></th>` under every column.
        s.push_str("<tr><th></th><th>Behavior</th>");
        for (_, label) in active[0].values.iter() {
            let _ = write!(s, "<th>{label}</th>");
        }
        s.push_str("<th>Notes</th></tr>\n");
    }
    s.push_str("</thead>\n<tbody>\n");

    for b in behaviors {
        let cases = coverage.get(b.id.as_str());
        let mut cells = Vec::with_capacity(combos.len());
        for combo in &combos {
            // A behavior can declare the scenario values it arises under at all. Without
            // this, a Compose-only behavior showed `·` under `img`/`dkr` — indistinguishable
            // from an untested gap, when the question does not arise.
            if combo.iter().any(|(dim, value)| {
                b.scenario_applicability
                    .get(*dim)
                    .is_some_and(|allowed| !allowed.iter().any(|a| a == value))
            }) {
                cells.push(Cell::NotApplicable);
                continue;
            }
            let hits: Vec<bool> = cases
                .into_iter()
                .flatten()
                .filter(|(case, _)| {
                    combo
                        .iter()
                        .all(|(k, v)| case.scenario_context.get(*k).map(String::as_str) == Some(*v))
                })
                .map(|(_, live)| *live)
                .collect();
            cells.push(match hits.iter().any(|l| *l) {
                _ if hits.is_empty() => Cell::Unchecked,
                live => covered_verdict(b, live),
            });
        }
        let roll = cells
            .iter()
            .copied()
            .find(|c| !matches!(c, Cell::Unchecked | Cell::NotApplicable))
            .unwrap_or(Cell::Unchecked);
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td>",
            roll.glyph(),
            first_sentence(&b.statement)
        );
        for c in &cells {
            let _ = write!(s, "<td>{}</td>", c.glyph());
        }
        let _ = writeln!(s, "<td>{}</td></tr>", notes_cell(b, waivers));
    }
    s.push_str("</tbody>\n</table>\n");
    s
}

/// The statement's first sentence — the full text is the registry's job, not the page's.
fn first_sentence(statement: &str) -> String {
    const MAX: usize = 88;
    let trimmed = statement.trim();
    let cut = trimmed.find(". ").map(|i| i + 1).unwrap_or(trimmed.len());
    let mut s = trimmed[..cut].trim_end_matches('.').trim().to_string();
    if s.chars().count() > MAX {
        // Cut on a word boundary; the full statement lives in the registry and the
        // page is a scanning surface, not a substitute for it.
        let mut end = 0;
        for (i, _) in s.char_indices().take(MAX) {
            end = i;
        }
        let boundary = s[..end].rfind(' ').unwrap_or(end);
        s.truncate(boundary);
        s.push('…');
    }
    html_escape(&s)
}

/// Issue links out of `notes`, so a reader can follow a divergence to its tracking
/// without the page restating the rationale.
fn notes_cell(b: &crate::model::BehaviorUnit, waivers: &BTreeMap<&str, &str>) -> String {
    let notes = b.notes.as_deref().unwrap_or("");
    let mut refs: Vec<String> = Vec::new();
    for (i, c) in notes.char_indices() {
        if c != '#' {
            continue;
        }
        let digits: String = notes[i + 1..]
            .chars()
            .take_while(|d| d.is_ascii_digit())
            .collect();
        if digits.len() >= 3 {
            let r = format!("#{digits}");
            if !refs.contains(&r) {
                refs.push(r);
            }
        }
    }
    refs.truncate(2);

    // A deliberate difference should be backed by a waiver: a waiver is verified to keep
    // reproducing and fails as STALE when the difference stops, so it is what stops a
    // characterization outliving the thing it characterized. Showing which waiver — and
    // flagging a deliberate difference that has none — is what makes the row auditable
    // rather than merely labelled.
    let mut out = Vec::new();
    match waivers.get(b.id.as_str()) {
        Some(id) => out.push(format!("<code>{id}</code>")),
        None if matches!(b.decision, Decision::IntentionalDivergence) => {
            out.push("<strong>no waiver</strong>".to_string());
        }
        None => {}
    }
    out.extend(refs);
    out.join(" ")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The pin for a revision KIND. Selecting by id substring silently made the spec pin
/// read `0.87.0` (the oracle's), because `rev-spec-…` does not contain "oracle" but
/// neither does `rev-cli-surface-…`, which sorts first.
fn pin_of(registry: &Registry, kind: RevisionKind) -> String {
    registry
        .revisions
        .iter()
        .find(|r| r.kind == kind)
        .map(|r| r.pin.clone())
        .unwrap_or_else(|| "(unpinned)".into())
}

fn header(registry: &Registry) -> String {
    let total = registry.behaviors.len();
    let deacon_only = registry
        .behaviors
        .iter()
        .filter(|b| {
            matches!(b.decision, Decision::DeaconExtension)
                || matches!(b.reference, ReferenceStatus::NotApplicable)
        })
        .count();
    let comparable = total - deacon_only;
    let deliberate = registry
        .behaviors
        .iter()
        .filter(|b| matches!(b.decision, Decision::IntentionalDivergence))
        .count();
    let behind = registry
        .behaviors
        .iter()
        .filter(|b| {
            matches!(b.reference, ReferenceStatus::Divergent)
                && !matches!(b.decision, Decision::IntentionalDivergence)
        })
        .count();
    let same = comparable.saturating_sub(deliberate + behind);

    format!(
        r#"# Does deacon behave like the DevContainers CLI?

<!-- GENERATED FILE — do not edit.
     Regenerate with: cargo run -q -p deacon-conformance -- parity-page --write -->

Compared against **`@devcontainers/cli` {oracle}** and the [containers.dev spec]\
(https://github.com/devcontainers/spec) at commit `{spec}`.

**Of {comparable} behaviors that both tools implement, {same} match.**
{deliberate} differ deliberately, {behind} are differences we intend to remove.
A further {deacon_only} are deacon-only, with nothing to compare against.

## How to read this

| | Meaning |
|---|---|
| ✅ | Same as the CLI, and checked against it in this scenario |
| ◐ | Believed the same, but only deacon-side evidence here — **never compared** |
| ⚠️ | Differs on purpose |
| ❌ | Differs, and we intend to fix it |
| 🔵 | deacon-only; the CLI has no equivalent |
| · | **Not checked yet** — a real gap |
| *(blank)* | Does not arise in this scenario |

A row's Notes name the **waiver** backing a deliberate difference, or say **no waiver**
where a deliberate difference has none. A waiver is verified to keep reproducing and fails
as *stale* when the difference stops, so it is what prevents a characterization outliving
the thing it characterized — a `⚠️` row with no waiver is one nothing will ever re-examine.

Columns are the scenarios a behavior was checked in: the configuration's shape
(**img**age / **dkr** Dockerfile / **cmp** Compose) crossed with how many Features it
declares (**–** none / **1** one / **many** several / **deps** several with dependency
ordering / **lock** with a lockfile).

**A column only appears where that scenario is possible.** `outdated` never resolves a
dependency graph, so it has no **deps** column at all rather than a column of `·` implying
an untested hole. So a `·` means genuinely not yet checked.

A cell says a case exercised that scenario — not that the case's assertions were strong.
The leading glyph rolls the row up; where a row is mostly `·`, that is the honest signal.

A **blank** cell means the behavior does not arise in that scenario at all — deriving a
Compose project name has no meaning under an image configuration. Blank rather than a
glyph, because there is no question there to answer.

So every `·` is a real gap: a scenario this behavior COULD be checked in and has not been.
"#,
        oracle = pin_of(registry, RevisionKind::Oracle),
        spec = pin_of(registry, RevisionKind::Spec),
    )
}

fn footer() -> String {
    "\n---\n\nThe full conformance record — the three-axis disposition behind each row, \
     the waivers, and the scenario-coverage accounting — lives in `conformance/registry/` \
     and `conformance/RULES.md`. This page is generated from it.\n"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        BehaviorUnit, Decision, ExpectedObservable, ReferenceStatus, SpecStatus, TestCase,
    };

    fn behavior() -> BehaviorUnit {
        BehaviorUnit {
            id: "bhv-x".into(),
            area: "up".into(),
            statement: "does a thing".into(),
            applicability: Vec::new(),
            spec: SpecStatus::Conformant,
            reference: ReferenceStatus::Aligned,
            decision: Decision::FollowSpec,
            notes: None,
            scenario_applicability: Default::default(),
        }
    }

    fn case_with(assertion: serde_json::Value) -> TestCase {
        let mut case = TestCase {
            id: "case-x".into(),
            behaviors: vec!["bhv-x".into()],
            ..TestCase::default()
        };
        case.oracle_type = Some(OracleType::LiveDifferential);
        case.scenario_context
            .insert("sdim-operation".into(), "up".into());
        case.scenario_context
            .insert("sdim-config-source".into(), "image".into());
        case.scenario_context
            .insert("sdim-features".into(), "single".into());
        case.expected = vec![ExpectedObservable {
            channel: "chan-stdout".into(),
            operation: None,
            assertion: Some(assertion),
        }];
        case
    }

    /// The rendered rows only — the legend necessarily contains every glyph, so
    /// asserting over the whole page is always true and tests nothing.
    fn rows_of(page: &str) -> String {
        page.lines()
            .skip_while(|l| !l.starts_with("### "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_with(assertion: serde_json::Value) -> String {
        let mut reg = Registry {
            behaviors: vec![behavior()],
            cases: vec![case_with(assertion)],
            ..Registry::default()
        };
        reg.cases[0].id = "case-x".into();
        render(&reg)
    }

    /// A case whose only assertion cannot fail must not colour a cell — it reports
    /// coverage while proving nothing, and `·` already means "not checked here".
    #[test]
    fn a_vacuous_case_does_not_colour_a_cell() {
        let page = rows_of(&render_with(serde_json::json!({"jsonSubset": {}})));
        assert!(
            !page.contains("✅") && !page.contains("◐"),
            "a vacuous assertion must leave the row unchecked:\n{page}"
        );
    }

    /// A scenario value the applicability RULES exclude for an operation must not render
    /// a column. Such a column can only ever be `·`, which reads as "we have not tested
    /// this" when the truth is "this cannot happen" — making coverage look worse than it
    /// is, exactly as damaging as making it look better.
    #[test]
    fn a_rule_excluded_value_renders_no_column() {
        use crate::model::Condition;
        use crate::scenario::{ApplicabilityRule, ScenarioDimension, ScenarioDimensionKind};

        let mut reg = Registry {
            behaviors: vec![behavior()],
            cases: vec![case_with(serde_json::json!({"equals": 0}))],
            ..Registry::default()
        };
        reg.scenario = vec![
            ScenarioDimension {
                id: "sdim-operation".into(),
                kind: ScenarioDimensionKind::Scenario,
                description: "operation".into(),
                values: vec!["up".into()],
            },
            ScenarioDimension {
                id: "sdim-features".into(),
                kind: ScenarioDimensionKind::Scenario,
                description: "features".into(),
                values: vec!["none".into(), "single".into(), "lockfile".into()],
            },
        ];
        reg.applicability = vec![ApplicabilityRule {
            id: "rule-x".into(),
            excludes: vec![
                Condition {
                    dimension: "sdim-operation".into(),
                    values: vec!["up".into()],
                },
                Condition {
                    dimension: "sdim-features".into(),
                    values: vec!["lockfile".into()],
                },
            ],
            ground: "the operation never reads a lockfile".into(),
        }];

        let page = render(&reg);
        assert!(
            page.contains("<th>1</th>"),
            "a permitted value must still render:\n{page}"
        );
        assert!(
            !page.contains("<th>lock</th>"),
            "a rule-excluded value must render no column:\n{page}"
        );
    }

    /// A weak-but-real assertion still counts: `{"features": {}}` requires the key to
    /// exist, so a document omitting it fails. Rejecting it here would make the page a
    /// style critic rather than an honest report.
    #[test]
    fn a_presence_only_case_still_colours_a_cell() {
        let page = rows_of(&render_with(
            serde_json::json!({"jsonSubset": {"features": {}}}),
        ));
        assert!(
            page.contains("✅"),
            "a presence-only assertion is real coverage:\n{page}"
        );
    }
}

/// One untested `(behavior, scenario)` cell — a `·` on the page, and a unit of work.
///
/// The parity matrix doubles as a queue. A dot names something concrete to go and do:
/// author a case for this behavior in this scenario, run it, and learn whether deacon
/// matches. That is a better driver than the pairwise obligation counts, which say how
/// many abstract value-pairs remain uncovered but never what to test.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCell {
    pub area: String,
    pub behavior: String,
    /// The scenario assignment, e.g. `{"sdim-config-source": "compose"}`.
    pub scenario: std::collections::BTreeMap<String, String>,
    /// This behavior's other scenarios that ARE covered. An empty list means the
    /// behavior has no evidence at all anywhere, which is a different and larger
    /// problem than one uncovered scenario.
    pub covered_elsewhere: usize,
    /// Whether any covering case anywhere compares against the reference. A behavior
    /// evidenced only deacon-side is worth more attention than one already differentially
    /// tested in a neighbouring scenario.
    pub differentially_tested: bool,
    /// Whether a waiver stands behind this behavior. A waiver IS evidence — it is
    /// verified to keep reproducing and fails as stale when it stops — but it is scoped
    /// to a difference, not to a scenario, so it never fills a cell. Without this the
    /// queue reported a waiver-backed behavior as having no evidence at all.
    pub waiver_backed: bool,
}

/// Enumerate every `·` on the page, in the order the page renders them.
///
/// Deliberately excludes blanks (the behavior does not arise there) and every non-dot
/// cell, so the result is exactly the open work.
pub fn open_cells(registry: &Registry) -> Vec<OpenCell> {
    let mut coverage: BTreeMap<&str, Vec<(&TestCase, bool)>> = BTreeMap::new();
    for case in &registry.cases {
        if !case.expected.is_empty()
            && case.expected.iter().all(|e| {
                e.assertion
                    .as_ref()
                    .is_some_and(crate::model::is_vacuous_assertion)
            })
        {
            continue;
        }
        let live = matches!(case.oracle_type, Some(OracleType::LiveDifferential));
        for b in &case.behaviors {
            coverage.entry(b.as_str()).or_default().push((case, live));
        }
    }

    let waived: BTreeSet<&str> = registry
        .waivers
        .iter()
        .flat_map(|w| w.behaviors.iter().map(String::as_str))
        .collect();

    let model = ScenarioModel::new(&registry.scenario, &registry.applicability);
    let mut areas: BTreeMap<&str, Vec<&crate::model::BehaviorUnit>> = BTreeMap::new();
    for b in &registry.behaviors {
        areas.entry(b.area.as_str()).or_default().push(b);
    }

    let mut out = Vec::new();
    for behaviors in areas.values() {
        let operations: BTreeSet<&str> = behaviors
            .iter()
            .filter_map(|b| coverage.get(b.id.as_str()))
            .flatten()
            .filter_map(|(case, _)| case.scenario_context.get("sdim-operation"))
            .map(String::as_str)
            .collect();
        let mut permitted: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for op in &operations {
            for (dim, values) in model.applicable_dimensions(op) {
                permitted
                    .entry(dim.id.as_str())
                    .or_default()
                    .extend(values.iter().copied());
            }
        }
        let active: Vec<ActiveDim<'_>> = DIMS
            .iter()
            .filter_map(|d| {
                let allowed = permitted.get(d.key)?;
                let values: Vec<(&str, &str)> = d
                    .values
                    .iter()
                    .filter(|(value, _)| allowed.contains(value))
                    .copied()
                    .collect();
                (values.len() > 1).then_some(ActiveDim { key: d.key, values })
            })
            .collect();
        if active.is_empty() {
            continue;
        }

        let mut combos: Vec<Vec<(&str, &str)>> = vec![Vec::new()];
        for dim in &active {
            let mut next = Vec::new();
            for base in &combos {
                for (value, _) in dim.values.iter() {
                    let mut c = base.clone();
                    c.push((dim.key, value));
                    next.push(c);
                }
            }
            combos = next;
        }

        for b in behaviors {
            let cases = coverage.get(b.id.as_str());
            let covered = cases.map_or(0, |c| c.len());
            let differential = cases.is_some_and(|c| c.iter().any(|(_, live)| *live));
            for combo in &combos {
                if combo.iter().any(|(dim, value)| {
                    b.scenario_applicability
                        .get(*dim)
                        .is_some_and(|allowed| !allowed.iter().any(|a| a == value))
                }) {
                    continue; // does not arise
                }
                let exercised = cases.into_iter().flatten().any(|(case, _)| {
                    combo
                        .iter()
                        .all(|(k, v)| case.scenario_context.get(*k).map(String::as_str) == Some(*v))
                });
                if exercised {
                    continue;
                }
                out.push(OpenCell {
                    area: b.area.clone(),
                    behavior: b.id.clone(),
                    scenario: combo
                        .iter()
                        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                        .collect(),
                    covered_elsewhere: covered,
                    differentially_tested: differential,
                    waiver_backed: waived.contains(b.id.as_str()),
                });
            }
        }
    }
    out
}
