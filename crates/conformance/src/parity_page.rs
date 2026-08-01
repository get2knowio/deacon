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
        let active: Vec<&Dim> = DIMS
            .iter()
            .filter(|d| permitted.get(d.key).is_some_and(|vs| vs.len() > 1))
            .collect();

        let _ = writeln!(out, "\n### {}\n", area_title(area));

        if active.is_empty() {
            // No scenario evidence to grid: a flat list is honest and smaller.
            out.push_str("| | Behavior | Notes |\n|---|---|---|\n");
            for b in behaviors {
                let live = coverage
                    .get(b.id.as_str())
                    .is_some_and(|cs| cs.iter().any(|(_, l)| *l));
                let _ = writeln!(
                    out,
                    "| {} | {} | {} |",
                    covered_verdict(b, live).glyph(),
                    first_sentence(&b.statement),
                    notes_cell(b)
                );
            }
            continue;
        }

        out.push_str(&grid(&active, behaviors, &coverage));
    }

    out.push_str(&footer());
    out
}

fn grid(
    active: &[&Dim],
    behaviors: &[&crate::model::BehaviorUnit],
    coverage: &BTreeMap<&str, Vec<(&TestCase, bool)>>,
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
            .find(|c| *c != Cell::Unchecked)
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
        let _ = writeln!(s, "<td>{}</td></tr>", notes_cell(b));
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
fn notes_cell(b: &crate::model::BehaviorUnit) -> String {
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
    refs.truncate(3);
    refs.join(" ")
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
| · | Not checked in this scenario |

Columns are the scenarios a behavior was checked in: the configuration's shape
(**img**age / **dkr** Dockerfile / **cmp** Compose) crossed with how many Features it
declares (**–** none / **1** one / **many** several / **deps** several with dependency
ordering / **lock** with a lockfile). Areas only show the columns their evidence varies.

A cell says a case exercised that scenario — not that the case's assertions were strong.
The leading glyph rolls the row up; where a row is mostly `·`, that is the honest signal.
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
