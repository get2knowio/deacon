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
//! Each cell carries how strongly the row was **checked in that scenario**, and nothing
//! else — an earlier draft used ✅ to mean "same as reference" in a status column and
//! "checked against the reference" in the cells, so a deliberately-divergent row rendered
//! as a line of green.
//!
//! ## One symbol, one axis
//!
//! The record has three axes (spec × reference × decision) and the page used to fold them
//! into a single glyph. Every fold of that kind has so far turned out to assert more than
//! the record supports — most sharply, "the two tools differ" rendered as ❌ *"we intend
//! to fix it"* for six behaviors where deacon matches the spec and the REFERENCE deviates.
//!
//! So the axes are unfolded: a **spec** column and a **CLI** column say what deacon is
//! measured against and whether it matches, the leading glyph says what we decided, and
//! the grid cells say only how well it has been checked. Some combinations are then
//! redundant by construction — a behavior sourced from observing the CLI can hardly show
//! `CLI ?` — and that is the intended trade: a redundant true cell costs a reader nothing,
//! a collapsed one costs them the distinction between our defect and theirs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::load::Registry;
use crate::model::{Decision, OracleType, ReferenceStatus, RevisionKind, SpecStatus, TestCase};
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
    /// A difference we intend to remove — deacon is the nonconformant side. Which side
    /// is at fault is NOT inferable from "the two differ": see [`rollup`].
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

/// The row's leading glyph: what we DECIDED about this behavior, plus whether anything
/// has checked it.
///
/// It deliberately does not say who deviates when the two tools differ — that is what the
/// `spec` and `CLI` columns carry, one axis each, unfolded rather than collapsed. Folding
/// them in is what made a divergence read as deacon's fault: R5 lets `follow-spec` stand
/// only on `spec: conformant`, so every behavior this page has ever rendered ❌ was one
/// where deacon matches the spec and the REFERENCE deviates from it. Only a
/// `nonconformant` spec axis puts deacon on the wrong side, and no behavior carries one.
fn rollup(b: &crate::model::BehaviorUnit, live: bool) -> Cell {
    match (b.decision, b.reference, b.spec) {
        (Decision::DeaconExtension, _, _) | (_, ReferenceStatus::NotApplicable, _) => {
            Cell::DeaconOnly
        }
        (Decision::IntentionalDivergence, _, _) => Cell::DiffersOnPurpose,
        (_, _, SpecStatus::Nonconformant) => Cell::DiffersFixing,
        _ if live => Cell::SameVerified,
        _ => Cell::SameUnverified,
    }
}

/// A grid cell: how strong the evidence is IN THIS SCENARIO, and nothing else.
///
/// Cells used to repeat the row's disposition, so a deliberate difference painted ⚠️
/// across every scenario it was covered in — including ones nothing had run. That reads
/// as breadth of checking when it is breadth of assertion.
fn evidence(live: bool) -> Cell {
    if live {
        Cell::SameVerified
    } else {
        Cell::SameUnverified
    }
}

/// One axis rendered as its own column: `✔` deacon matches it, `✘` it deviates from
/// deacon, `?` unsettled, blank where the axis does not apply.
fn spec_mark(s: SpecStatus) -> &'static str {
    match s {
        SpecStatus::Conformant => "✔",
        SpecStatus::Nonconformant => "✘",
        SpecStatus::Unspecified => "?",
        SpecStatus::NotApplicable => "",
    }
}

fn reference_mark(r: ReferenceStatus) -> &'static str {
    match r {
        ReferenceStatus::Aligned => "✔",
        ReferenceStatus::Divergent => "✘",
        ReferenceStatus::Unknown => "?",
        ReferenceStatus::NotApplicable => "",
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

/// Does this case check anything at all?
///
/// A case whose every declared assertion is vacuous checks nothing, so it must not
/// colour a cell or count as evidence. `·` already means "not checked in this
/// scenario", and a check that cannot fail IS not checking it. V37 makes such a case
/// unauthorable; this keeps the page's claim true by construction rather than by that
/// gate holding.
fn checks_anything(case: &TestCase) -> bool {
    case.expected.is_empty()
        || !case.expected.iter().all(|e| {
            e.assertion
                .as_ref()
                .is_some_and(crate::model::is_vacuous_assertion)
        })
}

/// Did this case OBSERVE the reference implementation?
///
/// The single definition of "verified against the CLI", used by both the grid and the
/// headline. They must not drift apart: the headline previously counted the recorded
/// three-axis disposition — a claim — while the grid counted evidence, so a behavior
/// asserting `reference: aligned` that nothing had ever run scored as parity. One
/// predicate, one standard, or the page speaks in two voices.
///
/// A snapshot counts: `conformance-snapshot refresh` runs the reference and commits
/// what it observed. A `spec-expectation` case does not — it compares deacon to a
/// written assertion and never invokes the CLI.
fn observed_the_reference(case: &TestCase) -> bool {
    matches!(
        case.oracle_type,
        Some(OracleType::LiveDifferential) | Some(OracleType::Snapshot)
    )
}

/// Render the page. Deterministic: same registry in, same bytes out.
pub fn render(registry: &Registry) -> String {
    // behavior -> set of (scenario key tuple, live?) drawn from its covering cases.
    let mut coverage: BTreeMap<&str, Vec<(&TestCase, bool)>> = BTreeMap::new();
    for case in &registry.cases {
        if !checks_anything(case) {
            continue;
        }
        let live = observed_the_reference(case);
        for b in &case.behaviors {
            coverage.entry(b.as_str()).or_default().push((case, live));
        }
    }

    let mut areas: BTreeMap<&str, Vec<&crate::model::BehaviorUnit>> = BTreeMap::new();
    for b in &registry.behaviors {
        areas.entry(b.area.as_str()).or_default().push(b);
    }

    let model = ScenarioModel::new(&registry.scenario, &registry.applicability);

    // A waiver is LIVE only if some case tolerates it via `allowedDifferences` — that is
    // what the declarative runner consumes and what reports it stale when the difference
    // stops reproducing. One nothing names is enforced by nothing.
    let consumed: BTreeSet<&str> = registry
        .cases
        .iter()
        .flat_map(|c| c.allowed_differences.iter())
        .filter_map(|d| d.waiver_id.as_deref())
        .collect();
    let waivers: BTreeMap<&str, (&str, bool)> = registry
        .waivers
        .iter()
        .flat_map(|w| {
            let live = consumed.contains(w.id.as_str());
            w.behaviors
                .iter()
                .map(move |b| (b.as_str(), (w.id.as_str(), live)))
        })
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
            out.push_str("| | Behavior | spec | CLI | Notes |\n|---|---|---|---|---|\n");
            for b in behaviors {
                // No covering case at all is `·`, not a verdict. Falling through to
                // `rollup` here rendered ◐ — "believed the same" — for a behavior
                // nothing has ever exercised, which is a claim, not a gap.
                let cell = match coverage.get(b.id.as_str()) {
                    None => Cell::Unchecked,
                    Some(cs) => rollup(b, cs.iter().any(|(_, l)| *l)),
                };
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} |",
                    cell.glyph(),
                    first_sentence(&b.statement),
                    spec_mark(b.spec),
                    reference_mark(b.reference),
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
    waivers: &BTreeMap<&str, (&str, bool)>,
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
        s.push_str(
            "<tr><th rowspan=\"2\"></th><th rowspan=\"2\">Behavior</th>\
             <th rowspan=\"2\">spec</th><th rowspan=\"2\">CLI</th>",
        );
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
        s.push_str("<tr><th></th><th>Behavior</th><th>spec</th><th>CLI</th>");
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
                live => evidence(live),
            });
        }
        // The rollup comes from the RECORD, not from scanning the cells: cells now carry
        // evidence strength only, so deriving it from them would report every covered row
        // as ✅ regardless of what we decided about it.
        let anything_covered = cells
            .iter()
            .any(|c| !matches!(c, Cell::Unchecked | Cell::NotApplicable));
        let roll = if anything_covered {
            rollup(b, cells.iter().any(|c| matches!(c, Cell::SameVerified)))
        } else {
            Cell::Unchecked
        };
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td>",
            roll.glyph(),
            first_sentence(&b.statement),
            spec_mark(b.spec),
            reference_mark(b.reference)
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
fn notes_cell(b: &crate::model::BehaviorUnit, waivers: &BTreeMap<&str, (&str, bool)>) -> String {
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

    // A deliberate difference is only auditable if something re-checks that it still
    // reproduces — that is the whole value of a waiver, which fails as STALE when the
    // difference stops. But EXISTING is not the same as being checked. A waiver becomes
    // live by a case tolerating it via `allowedDifferences`, which the declarative runner
    // consumes and reports stale when unconsumed. A waiver no case names is enforced by
    // nothing: the `corpus_case` scope most of them carry was driven by the four corpus
    // carriers deleted in 023, and no live binary has called `corpus_case`/`stale_among`
    // against the real waiver set since. So the row must distinguish the two, or naming
    // the waiver id restates the earlier defect one column over — rendering a claim in
    // the place a reader reads evidence.
    let mut out = Vec::new();
    match waivers.get(b.id.as_str()) {
        Some((id, true)) => out.push(format!("<code>{id}</code>")),
        Some((id, false)) => out.push(format!("<code>{id}</code> <strong>(unchecked)</strong>")),
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
    // Which behaviors has anything actually compared against the reference? Same
    // predicate the grid uses, so the headline and the cells below it cannot disagree.
    let observed: BTreeSet<&str> = registry
        .cases
        .iter()
        .filter(|c| checks_anything(c) && observed_the_reference(c))
        .flat_map(|c| c.behaviors.iter().map(String::as_str))
        .collect();

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

    // Split each bucket by whether its claim about the reference rests on evidence.
    // `same` is still a residual — a behavior lands there by being in neither
    // divergence bucket — which is exactly why it must not be reported as a
    // measurement without saying how much of it was measured.
    let mut counts = [[0usize; 2]; 4];
    for b in &registry.behaviors {
        if matches!(b.decision, Decision::DeaconExtension)
            || matches!(b.reference, ReferenceStatus::NotApplicable)
        {
            continue;
        }
        // Same order of tests as `rollup`, so the headline cannot claim a fault the row
        // does not show. "The two differ" is split by WHOSE deviation it is: only a
        // nonconformant spec axis is deacon's.
        let bucket = if matches!(b.decision, Decision::IntentionalDivergence) {
            1
        } else if matches!(b.spec, SpecStatus::Nonconformant) {
            3
        } else if matches!(b.reference, ReferenceStatus::Divergent) {
            2
        } else {
            0
        };
        counts[bucket][usize::from(observed.contains(b.id.as_str()))] += 1;
    }
    let [
        [same_asserted, same],
        [delib_asserted, deliberate],
        [spec_backed_asserted, spec_backed],
        [behind_asserted, behind],
    ] = counts;
    let unverified = same_asserted + delib_asserted + spec_backed_asserted + behind_asserted;

    format!(
        r#"# Does deacon behave like the DevContainers CLI?

<!-- GENERATED FILE — do not edit.
     Regenerate with: cargo run -q -p deacon-conformance -- parity-page --write -->

Compared against **`@devcontainers/cli` {oracle}** and the [containers.dev spec]\
(https://github.com/devcontainers/spec) at commit `{spec}`.

**Of {comparable} behaviors that both tools implement, {same} are verified to match.**
{deliberate} are verified to differ deliberately, and {spec_backed} differ because deacon
follows the spec where the CLI does not — those are the CLI's deviation, not work we owe.
{behind} are differences where deacon is the nonconformant side. A further {deacon_only}
are deacon-only, with nothing to compare against.

**{unverified} more have never been compared against the CLI at all** — {same_asserted} assumed
to match, {delib_asserted} assumed to differ on purpose, {spec_backed_asserted} assumed to be the
CLI deviating from the spec, {behind_asserted} assumed to be deacon's fault. These are claims, not
measurements: nothing has run the reference for them, so each could turn out to be any of the
four. They are listed separately rather than folded into the totals above, because a claim nobody
has checked is not evidence of parity — and a page that counted it as such would improve every
time someone asserted something new.

## How to read this

Two columns carry the two things deacon is measured against, one axis each — **spec** is
the containers.dev specification, **CLI** is the reference implementation:

| | **spec** / **CLI** column |
|---|---|
| ✔ | deacon matches it |
| ✘ | it and deacon differ |
| ? | unsettled — the spec is silent, or nothing has compared us to the CLI |
| *(blank)* | no equivalent exists to compare against |

Read them together: `✔ ✘` is a difference where **the spec is on deacon's side and the
CLI is the one deviating** — it is not work we owe. `✘ ✘` would be deacon being wrong; no
behavior currently carries it. `? ✘` is a difference the spec does not settle either way,
which is where a deliberate decision — and only there, a waiver — belongs.

The leading glyph is what we **decided**, and the grid cells are how strongly it has been
**checked**. They are kept apart on purpose: a decision repeated across every scenario
column reads as breadth of testing when it is breadth of assertion.

| | Leading glyph — our decision |
|---|---|
| ✅ | Follow the spec / match the CLI, and something has compared us |
| ◐ | The same intent, but only deacon-side evidence — **never compared** |
| ⚠️ | Differs on purpose |
| ❌ | deacon is the nonconformant side, and we intend to fix it |
| 🔵 | deacon-only; the CLI has no equivalent |
| · | **Not checked yet** — a real gap |

| | Grid cell — evidence in that scenario |
|---|---|
| ✅ | Checked against the CLI here |
| ◐ | Exercised here, but only deacon-side evidence |
| · | **Not checked yet** — a real gap |
| *(blank)* | Does not arise in this scenario |

A row's Notes name the **waiver** backing a deliberate difference. A waiver's value is
that it self-invalidates: it is re-checked against the reference and fails as *stale* the
moment the difference stops reproducing, so a characterization cannot outlive the thing it
characterized.

That only holds while something re-checks it. A waiver becomes live by a test case
tolerating it; one no case names is enforced by nothing, and is marked **(unchecked)**
here. **no waiver** means a deliberate difference has none at all. Both mean the same
thing for a reader: that difference is asserted, and nothing would notice if the CLI
changed to match us tomorrow.

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

    /// The headline must count what was OBSERVED, not what the registry CLAIMS.
    ///
    /// It previously derived every total from the three-axis disposition, so a behavior
    /// recorded `reference: aligned` scored as parity even though nothing had ever run
    /// the reference for it. That let the page improve whenever someone asserted
    /// something new, and split it against its own grid, which counts evidence. The two
    /// now share `observed_the_reference`; this pins the header side of that.
    #[test]
    fn an_unobserved_claim_is_not_counted_as_verified_parity() {
        let mut reg = Registry {
            behaviors: vec![behavior()],
            cases: vec![case_with(serde_json::json!({"contains": "x"}))],
            ..Registry::default()
        };
        // The behavior claims alignment, but its only evidence never runs the reference.
        reg.cases[0].oracle_type = Some(OracleType::SpecExpectation);
        let page = render(&reg);
        assert!(
            page.contains("**Of 1 behaviors that both tools implement, 0 are verified to match.**"),
            "a spec-expectation case must not verify a parity claim:\n{page}"
        );
        assert!(
            page.contains("**1 more have never been compared against the CLI at all**"),
            "the unobserved claim must be reported as uncompared:\n{page}"
        );

        // Same registry, same disposition — only the evidence changes.
        reg.cases[0].oracle_type = Some(OracleType::LiveDifferential);
        let page = render(&reg);
        assert!(
            page.contains("**Of 1 behaviors that both tools implement, 1 are verified to match.**"),
            "running the reference is what must move the count:\n{page}"
        );
    }

    /// R5 permits `follow-spec` only on `spec: conformant`, so "the two tools differ"
    /// never by itself means deacon is wrong. Collapsing both axes into one glyph
    /// reported the CLI's deviation from the spec as deacon's defect — for every ❌ the
    /// page had ever rendered.
    #[test]
    fn a_difference_where_the_spec_backs_deacon_is_not_reported_as_deacons_fault() {
        let mut b = behavior();
        b.reference = ReferenceStatus::Divergent; // deacon and the CLI differ …
        b.spec = SpecStatus::Conformant; // … and the spec is on deacon's side.
        let reg = Registry {
            behaviors: vec![b],
            cases: vec![case_with(serde_json::json!({"contains": "x"}))],
            ..Registry::default()
        };
        let page = render(&reg);
        assert!(
            page.contains("0 are differences where deacon is the nonconformant side"),
            "the spec backs deacon here; nothing is owed:\n{page}"
        );
        assert!(
            page.contains("1 differ because deacon\nfollows the spec where the CLI does not"),
            "the difference must be attributed to the CLI:\n{page}"
        );
        let rows = rows_of(&page);
        assert!(!rows.contains('❌'), "no row is deacon's fault:\n{rows}");
        assert!(
            rows.contains("✔") && rows.contains("✘"),
            "both axes must be shown unfolded, one column each:\n{rows}"
        );

        // Flip only the spec axis: now deacon IS the nonconformant side.
        let mut b = behavior();
        b.reference = ReferenceStatus::Divergent;
        b.spec = SpecStatus::Nonconformant;
        let reg = Registry {
            behaviors: vec![b],
            ..reg
        };
        let page = render(&reg);
        assert!(
            page.contains("1 are differences where deacon is the nonconformant side"),
            "a nonconformant spec axis is the one thing that makes it ours:\n{page}"
        );
        assert!(rows_of(&page).contains('❌'), "{page}");
    }

    /// A waiver no case tolerates is enforced by nothing, and must not read as though it
    /// were. Naming the waiver id was itself a claim rendered where a reader reads
    /// evidence: the id proves a record was written, not that anything re-checks it.
    #[test]
    fn a_waiver_no_case_consumes_is_marked_unchecked() {
        use crate::model::{AllowedDifference, Expect, Scope, Waiver};
        let waiver = |id: &str| Waiver {
            id: id.into(),
            behaviors: vec!["bhv-x".into()],
            scope: Scope::CorpusCase {
                corpus: "c".into(),
                case: "k".into(),
            },
            expect: Expect::BothAccept {},
            rationale: "r".into(),
            added: "2026-01-01".into(),
            expires: "2027-01-01".into(),
            config: None,
        };
        let tolerance = |id: &str| AllowedDifference {
            behavior: "bhv-x".into(),
            context: Vec::new(),
            observable_path: "chan-stdout.x".into(),
            rationale: "r".into(),
            waiver_id: Some(id.into()),
            divergence_id: None,
        };
        let mut b = behavior();
        b.decision = Decision::IntentionalDivergence;
        let mut reg = Registry {
            behaviors: vec![b],
            cases: vec![case_with(serde_json::json!({"contains": "x"}))],
            waivers: vec![waiver("wvr-x")],
            ..Registry::default()
        };
        // Assert over the ROWS only: the legend explains the marker, so it necessarily
        // contains the literal `(unchecked)` and a whole-page assertion is always true.
        let rows = rows_of(&render(&reg));
        assert!(
            rows.contains("<code>wvr-x</code> <strong>(unchecked)</strong>"),
            "a waiver nothing consumes must be flagged:\n{rows}"
        );

        // Same waiver, same behavior — only a case tolerance is added.
        reg.cases[0].allowed_differences = vec![tolerance("wvr-x")];
        let rows = rows_of(&render(&reg));
        assert!(
            rows.contains("<code>wvr-x</code>") && !rows.contains("(unchecked)"),
            "a consumed waiver must render plain:\n{rows}"
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
