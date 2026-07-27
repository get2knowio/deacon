//! Byte-stable discovery reports — `target/discovery/queue.{json,md}`
//! (025-exploratory-parity-discovery, contracts/discovery-cli.md, quickstart.md § 2).
//!
//! ## Reporting never gates
//!
//! `discovery report`'s exit status reflects only whether the artifacts were **written**,
//! never what they say. A queue holding fifty untriaged findings still exits `0`. This is
//! the `coverage report` discipline extended to the discovery surface (FR-058): any
//! command whose status depends on its findings becomes a gate the moment someone wires
//! it into CI, and a stochastic gate makes green non-reproducible.
//!
//! ## Volume is reported even when nothing was found
//!
//! A campaign that found nothing and a campaign that never ran are completely different
//! facts (FR-062). The report therefore always carries `candidatesGenerated` /
//! `candidatesExecuted` per campaign, so "nothing found" can never be confused with
//! "nothing ran" — which would otherwise make a broken pipeline the most comfortable
//! possible state for the machinery to be in.
//!
//! ## Byte-stable
//!
//! No timestamps, no absolute paths, no map iteration order that is not explicitly
//! sorted. Two runs over the same data produce identical bytes, so the artifacts diff
//! cleanly and a change in the report means a change in the queue.
//!
//! ## Grouping is a view, never a merge
//!
//! Distinct signatures stay distinct findings even when they map to the same behavior
//! (FR-031); they are *reported* grouped. Merging them would destroy the ability to tell
//! whether a fix addressed one cause or all of them, so [`FindingGroup`] names the
//! findings that relate and changes nothing about them.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::mutate;
use super::queue::{Budget, Campaign, DiscoveryData, Finding, FindingState, PinnedInputSet};

/// The pins a finding is compared against to decide whether it is **pin-stale**.
///
/// A finding is a claim about a *specific* pinned pair of implementations. On a pin
/// change the claim is neither true nor false — it is unverified — so such findings are
/// listed for re-evaluation rather than carried forward, which would assert that a
/// difference observed against oracle *v0.86* still holds against *v0.87*
/// (contracts/findings-queue.md, "Pin invalidation").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentPins {
    /// The vendored schema revision pin.
    pub schema_pin: String,
    /// The vendored spec-prose revision pin.
    pub prose_pin: String,
    /// The recorded oracle version, or `None` when the registry records no oracle
    /// revision at all.
    ///
    /// `None` means **"oracle staleness is not decidable"**, not "the current oracle
    /// version is the empty string". The distinction is the whole point: comparing every
    /// campaign's real oracle version against `""` would mark the entire queue pin-stale
    /// for a registry that simply had not recorded an oracle yet, which reads as a
    /// catastrophic pin bump instead of as missing metadata.
    pub oracle_version: Option<String>,
}

impl CurrentPins {
    /// Resolve the current pins from a loaded registry.
    ///
    /// The schema and prose pins fall back to this crate's compiled-in constants, which
    /// are the same values the registry is validated against (V14); the oracle pin has no
    /// such constant, so its absence is represented rather than invented.
    pub fn from_registry(registry: &crate::load::Registry) -> CurrentPins {
        let pin_of = |kind: crate::model::RevisionKind| -> Option<String> {
            registry
                .revisions
                .iter()
                .find(|r| r.kind == kind)
                .map(|r| r.pin.clone())
        };
        CurrentPins {
            schema_pin: pin_of(crate::model::RevisionKind::Schema)
                .unwrap_or_else(|| crate::CURRENT_SCHEMA_PIN.to_string()),
            prose_pin: pin_of(crate::model::RevisionKind::Spec)
                .unwrap_or_else(|| crate::CURRENT_SPEC_PIN.to_string()),
            oracle_version: pin_of(crate::model::RevisionKind::Oracle),
        }
    }

    /// Whether `campaign` ran under a different pinned pair than the current one.
    ///
    /// An undecidable element (see [`oracle_version`](Self::oracle_version)) contributes
    /// nothing: "we cannot tell" must never be reported as "it differs".
    fn differs_from(&self, campaign: &Campaign) -> bool {
        let pins = &campaign.pinned_input_set;
        pins.schema_pin != self.schema_pin
            || pins.prose_pin != self.prose_pin
            || self
                .oracle_version
                .as_ref()
                .is_some_and(|current| &pins.oracle_version != current)
    }
}

/// One finding as the report renders it — enough to triage from, without the witness
/// payloads that would make the artifact unreadable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingSummary {
    /// `fnd-<hash8>`.
    pub id: String,
    /// The signature's channel.
    pub channel: String,
    /// The signature's observable path.
    pub path: String,
    /// The difference kind's wire spelling.
    pub kind: String,
    /// The value-shape class's wire spelling.
    pub value_shape_class: String,
    /// How many witnesses back it.
    pub witnesses: usize,
    /// Its classification, if triaged.
    pub classification: Option<String>,
    /// The campaign that first admitted it.
    pub first_observed: String,
    /// The most recent campaign that reproduced it — for `no-longer-reproducing`, this
    /// is *the campaign that last observed it*, which FR-033 requires to be reported
    /// rather than deleted along with the record.
    pub last_observed: String,
    /// The registry case carrying it, when promoted.
    pub promoted_to: Option<String>,
}

impl FindingSummary {
    fn of(finding: &Finding) -> FindingSummary {
        FindingSummary {
            id: finding.id.clone(),
            channel: finding.signature.channel.clone(),
            path: finding.signature.path.clone(),
            kind: finding.signature.kind.as_str().to_string(),
            value_shape_class: finding.signature.value_shape_class.as_str().to_string(),
            witnesses: finding.witnesses.len(),
            classification: finding.classification.map(|c| c.as_str().to_string()),
            first_observed: finding.first_observed.clone(),
            last_observed: finding.last_observed.clone(),
            promoted_to: finding.promoted_to.clone(),
        }
    }
}

/// Resolves a promoted finding to the behavior identities its case claims (**T073**).
///
/// Built from the registry rather than from the queue because a finding **never names a
/// behavior**. That is not an omission: FR-025 forbids a discovery program inventing a
/// behavior identity, so the only behavior a finding can honestly be grouped under is the
/// one a human already attached to it — by promoting it into a case that names behaviors.
/// Anything else would be the report asserting a mapping nobody reviewed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BehaviorIndex {
    /// `case-<id>` → the behaviors that case claims, in the case's own declaration order.
    case_behaviors: std::collections::BTreeMap<String, Vec<String>>,
}

impl BehaviorIndex {
    /// Index every registry case's behaviors.
    pub fn from_registry(registry: &crate::load::Registry) -> BehaviorIndex {
        BehaviorIndex {
            case_behaviors: registry
                .cases
                .iter()
                .map(|c| (c.id.clone(), c.behaviors.clone()))
                .collect(),
        }
    }

    /// The behaviors a finding is known to map to — empty unless it is promoted into a
    /// case that resolves.
    fn behaviors_of(&self, finding: &Finding) -> &[String] {
        finding
            .promoted_to
            .as_deref()
            .and_then(|case| self.case_behaviors.get(case))
            .map_or(&[][..], Vec::as_slice)
    }
}

/// How a [`FindingGroup`] was keyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupKind {
    /// A `bhv-` identity, resolved through a promoted finding's case. A *reviewed*
    /// mapping.
    Behavior,
    /// `channel ‖ path` — the observable location the findings share. Explicitly **not** a
    /// behavior claim: it is the strongest grouping available before a human has decided
    /// what these differences mean.
    ObservablePath,
}

/// Several distinct findings reported together (FR-031).
///
/// **Grouping is a view, never a merge.** FR-031 requires distinct signatures to stay
/// distinct findings even when they map to the same behavior, and permits reporting them
/// grouped. Merging would destroy the ability to tell whether a fix addressed one cause or
/// all of them; grouping without merging keeps both facts — the reviewer sees the
/// relationship, and each finding retains its own witnesses, its own classification, and
/// its own promotion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingGroup {
    /// The behavior id, or `channel ‖ path`.
    pub key: String,
    /// What the key is.
    pub kind: GroupKind,
    /// The findings sharing it — **≥ 2**, in the queue's own file order. A group of one is
    /// the finding itself and would only pad the report.
    pub findings: Vec<String>,
}

/// One campaign's volume line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignSummary {
    /// `cmp-<hash8>`.
    pub id: String,
    /// The recorded seed — the reproducibility input.
    pub seed: String,
    /// The tier's wire spelling.
    pub tier: String,
    /// The lane's wire spelling.
    pub lane: String,
    /// Candidates generated — reported even when nothing was found (FR-062).
    pub candidates_generated: u64,
    /// Candidates executed.
    pub candidates_executed: u64,
    /// Distinct signatures admitted.
    pub signatures_admitted: u64,
    /// Distinct signatures the admission cap suppressed — never silent (FR-034b).
    pub signatures_suppressed: u64,
    /// Whether the wall-clock budget ran out.
    pub budget_exhausted: bool,
}

impl CampaignSummary {
    fn of(campaign: &Campaign) -> CampaignSummary {
        CampaignSummary {
            id: campaign.id.clone(),
            seed: campaign.seed.clone(),
            tier: campaign.tier.as_str().to_string(),
            lane: campaign.lane.as_str().to_string(),
            candidates_generated: campaign.outcome.candidates_generated,
            candidates_executed: campaign.outcome.candidates_executed,
            signatures_admitted: campaign.outcome.signatures_admitted,
            signatures_suppressed: campaign.outcome.signatures_suppressed,
            budget_exhausted: campaign.outcome.budget_exhausted,
        }
    }
}

/// The queue report (quickstart.md § 2's five buckets).
///
/// `untriaged` is **counted**, not merely listed, which is the whole point of FR-029:
/// "nobody has looked yet" must never read as "nothing found".
///
/// `pinStale` is **cross-cutting**, not a sixth state: a pin-stale finding also appears
/// in whichever state bucket it is in. Making it a state would force a finding to stop
/// being `triaged` because a pin moved, discarding a reviewer's judgement over an event
/// that says nothing about the finding's cause.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueReport {
    /// Total findings in the queue.
    pub total: usize,
    /// Nobody has looked yet.
    pub untriaged: Vec<FindingSummary>,
    /// Classified, awaiting a decision.
    pub triaged: Vec<FindingSummary>,
    /// Split into children; an inert ancestor.
    pub split: Vec<FindingSummary>,
    /// Now carried by a real case, named.
    pub promoted: Vec<FindingSummary>,
    /// Stopped reproducing; names the campaign that last saw it.
    pub no_longer_reproducing: Vec<FindingSummary>,
    /// Observed under pins that no longer match; awaiting re-evaluation.
    pub pin_stale: Vec<FindingSummary>,
    /// Distinct findings that share a behavior or an observable path, reported together
    /// and **never merged** (FR-031). Sorted by key, so the artifact is byte-stable.
    pub groups: Vec<FindingGroup>,
    /// The campaign history, in file order.
    pub campaigns: Vec<CampaignSummary>,
    /// Signatures the admission cap suppressed across every campaign (FR-034b).
    ///
    /// Surfaced as a queue-level total, not only per campaign, because that is the number
    /// that tells a reviewer the queue is a *sample*: fifty untriaged findings beside a
    /// suppression count of zero means "this is everything", and beside a count of three
    /// hundred it means "this is what we could look at". A silent truncation would render
    /// both identically.
    pub signatures_suppressed: u64,
}

/// Build the report with no behavior mapping — grouping falls back to the observable path.
///
/// Pure: no I/O, no clock, no environment.
pub fn build_queue_report(data: &DiscoveryData, pins: &CurrentPins) -> QueueReport {
    build_queue_report_with_behaviors(data, pins, &BehaviorIndex::default())
}

/// Build the report, resolving promoted findings to the behaviors their cases claim
/// (**T073**).
///
/// Pure: no I/O, no clock, no environment.
pub fn build_queue_report_with_behaviors(
    data: &DiscoveryData,
    pins: &CurrentPins,
    behaviors: &BehaviorIndex,
) -> QueueReport {
    let mut report = QueueReport {
        total: data.findings.len(),
        campaigns: data.campaigns.iter().map(CampaignSummary::of).collect(),
        signatures_suppressed: data
            .campaigns
            .iter()
            .map(|c| c.outcome.signatures_suppressed)
            .sum(),
        ..QueueReport::default()
    };

    // Keyed collection for the FR-031 grouping view. A finding contributes to a behavior
    // group for EVERY behavior its case claims — a case covering two behaviors relates its
    // finding to both — and to its observable-path group regardless, so the relationship is
    // visible before anything is promoted as well as after.
    let mut behavior_groups: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut path_groups: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for finding in &data.findings {
        let summary = FindingSummary::of(finding);
        match finding.state {
            FindingState::Untriaged => report.untriaged.push(summary.clone()),
            FindingState::Triaged => report.triaged.push(summary.clone()),
            FindingState::Split => report.split.push(summary.clone()),
            FindingState::Promoted => report.promoted.push(summary.clone()),
            FindingState::NoLongerReproducing => report.no_longer_reproducing.push(summary.clone()),
        }
        // Pin staleness is judged against the campaign that LAST observed the finding:
        // that is the most recent evidence, and it is the run a re-evaluation would be
        // compared against. An unresolvable reference is D1's problem, not the
        // report's, so a dangling id simply does not mark the finding stale.
        if data
            .campaign(&finding.last_observed)
            .is_some_and(|c| pins.differs_from(c))
        {
            report.pin_stale.push(summary);
        }

        for behavior in behaviors.behaviors_of(finding) {
            behavior_groups
                .entry(behavior.clone())
                .or_default()
                .push(finding.id.clone());
        }
        path_groups
            .entry(format!(
                "{} {}",
                finding.signature.channel, finding.signature.path
            ))
            .or_default()
            .push(finding.id.clone());
    }

    // Only groups of two or more: a group of one is the finding itself.
    for (kind, source) in [
        (GroupKind::Behavior, behavior_groups),
        (GroupKind::ObservablePath, path_groups),
    ] {
        for (key, findings) in source {
            if findings.len() >= 2 {
                report.groups.push(FindingGroup {
                    key,
                    kind,
                    findings,
                });
            }
        }
    }

    report
}

/// Render the canonical, byte-stable JSON document (2-space, trailing newline).
pub fn render_json(report: &QueueReport) -> String {
    let mut out = serde_json::to_string_pretty(report)
        .unwrap_or_else(|e| unreachable!("queue-report serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Render the human-review Markdown document.
///
/// Deterministic by construction: every list is emitted in the queue's own file order and
/// nothing here reads a clock, an environment variable, or an absolute path.
pub fn render_md(report: &QueueReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _ = writeln!(out, "# Discovery findings queue\n");
    let _ = writeln!(
        out,
        "Findings are **candidates for assertions**, never assertions. Nothing here \
         reaches `certify`, and this report never gates.\n"
    );

    let _ = writeln!(out, "## Summary\n");
    let _ = writeln!(out, "| Bucket | Count |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| total | {} |", report.total);
    let _ = writeln!(out, "| untriaged | {} |", report.untriaged.len());
    let _ = writeln!(out, "| triaged | {} |", report.triaged.len());
    let _ = writeln!(out, "| split | {} |", report.split.len());
    let _ = writeln!(out, "| promoted | {} |", report.promoted.len());
    let _ = writeln!(
        out,
        "| no-longer-reproducing | {} |",
        report.no_longer_reproducing.len()
    );
    let _ = writeln!(out, "| pin-stale | {} |", report.pin_stale.len());
    let _ = writeln!(out, "| campaigns | {} |", report.campaigns.len());
    let _ = writeln!(
        out,
        "| signatures suppressed | {} |\n",
        report.signatures_suppressed
    );

    if report.signatures_suppressed > 0 {
        let _ = writeln!(
            out,
            "**This queue is a sample.** {} distinct signature(s) were observed and not \
             admitted, because their campaigns reached the admission cap. Read every count \
             above against that number: suppression is reported precisely so \"we found 25 \
             things\" can never be mistaken for \"we found many more than we can review\", \
             and a campaign that keeps hitting its cap is itself a signal that something \
             systemic is diverging (FR-034b).\n",
            report.signatures_suppressed
        );
    }

    if report.total == 0 {
        let _ = writeln!(
            out,
            "The queue is empty. Note what that does **not** say: it does not say the two \
             implementations agree. Check the campaign table below — a queue that is empty \
             because nothing ran looks exactly like a queue that is empty because nothing \
             differed.\n"
        );
    }

    for (title, findings, note) in [
        (
            "Untriaged",
            &report.untriaged,
            "Nobody has looked yet. This bucket is counted so it can never read as \
             \"nothing found\".",
        ),
        (
            "Triaged",
            &report.triaged,
            "Classified, awaiting a decision.",
        ),
        (
            "Split",
            &report.split,
            "Inert ancestors: they keep their witnesses as historical record, accept no \
             new ones, and surrender classification to their children.",
        ),
        (
            "Promoted",
            &report.promoted,
            "Carried by a real registry case, named below.",
        ),
        (
            "No longer reproducing",
            &report.no_longer_reproducing,
            "Retained, not deleted: the disappearance may mean a fix landed, or it may \
             mean the generator stopped reaching the input, and only the retained record \
             tells those apart.",
        ),
        (
            "Pin-stale",
            &report.pin_stale,
            "Observed under pins that no longer match. Re-evaluated by the next campaign, \
             never carried forward unverified. Cross-cutting: each also appears in its own \
             state bucket above.",
        ),
    ] {
        let _ = writeln!(out, "## {title} ({})\n", findings.len());
        let _ = writeln!(out, "{note}\n");
        if findings.is_empty() {
            let _ = writeln!(out, "_None._\n");
            continue;
        }
        let _ = writeln!(
            out,
            "| finding | channel | path | kind | shape | witnesses | classification | last observed | promoted to |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|");
        for f in findings {
            let _ = writeln!(
                out,
                "| `{}` | {} | `{}` | {} | {} | {} | {} | `{}` | {} |",
                f.id,
                f.channel,
                f.path,
                f.kind,
                f.value_shape_class,
                f.witnesses,
                f.classification.as_deref().unwrap_or("—"),
                f.last_observed,
                f.promoted_to
                    .as_deref()
                    .map(|c| format!("`{c}`"))
                    .unwrap_or_else(|| "—".to_string()),
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Related findings ({})\n", report.groups.len());
    let _ = writeln!(
        out,
        "Findings that share a behavior or an observable path. **Grouping is a view, never \
         a merge** (FR-031): each finding below keeps its own witnesses, its own \
         classification, and its own promotion, because merging them would destroy the \
         ability to tell whether a fix addressed one cause or all of them.\n"
    );
    let _ = writeln!(
        out,
        "A `behavior` key is a *reviewed* mapping — it exists only because a human promoted \
         a finding into a case naming that behavior. An `observable-path` key is not a \
         behavior claim; it is the strongest relationship available before anyone has \
         decided what these differences mean.\n"
    );
    let _ = writeln!(
        out,
        "One shape to read carefully: a **split lineage** appears as a group whose members \
         share a single signature, because a split separates witnesses rather than \
         signatures. That is the lineage, not several distinct causes — the causes are what \
         the reviewer separated them to record, one classification per child.\n"
    );
    if report.groups.is_empty() {
        let _ = writeln!(out, "_None._\n");
    } else {
        let _ = writeln!(out, "| key | kind | findings |");
        let _ = writeln!(out, "|---|---|---|");
        for group in &report.groups {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} |",
                group.key,
                match group.kind {
                    GroupKind::Behavior => "behavior",
                    GroupKind::ObservablePath => "observable-path",
                },
                group
                    .findings
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<String>>()
                    .join(", ")
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Campaigns ({})\n", report.campaigns.len());
    let _ = writeln!(
        out,
        "Volume is reported whether or not anything was found (FR-062): a campaign that \
         found nothing and a campaign that never ran are different facts.\n"
    );
    if report.campaigns.is_empty() {
        let _ = writeln!(out, "_None._\n");
    } else {
        let _ = writeln!(
            out,
            "| campaign | seed | tier | lane | generated | executed | admitted | suppressed | budget exhausted |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|");
        for c in &report.campaigns {
            let _ = writeln!(
                out,
                "| `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} |",
                c.id,
                c.seed,
                c.tier,
                c.lane,
                c.candidates_generated,
                c.candidates_executed,
                c.signatures_admitted,
                c.signatures_suppressed,
                c.budget_exhausted,
            );
        }
        let _ = writeln!(out);
    }

    out
}

/// Atomically write `queue.json` + `queue.md` into `dir`, returning the written paths in
/// a deterministic order.
pub fn write_queue_report(dir: &Path, report: &QueueReport) -> std::io::Result<Vec<PathBuf>> {
    let json = dir.join("queue.json");
    let md = dir.join("queue.md");
    crate::atomic_write(&json, &render_json(report))?;
    crate::atomic_write(&md, &render_md(report))?;
    Ok(vec![json, md])
}

// ---------------------------------------------------------------------------
// T039 / T123 — the campaign-outcome report
// ---------------------------------------------------------------------------

/// One campaign's own report (FR-061), rendered from its recorded outcome.
///
/// Distinct from [`QueueReport`], which is the standing triage queue across all campaigns.
/// This is what a single run emits — the document a reviewer reads to answer "what did
/// last night's campaign actually do?".
///
/// **Every field here is reported whether or not the campaign found anything** (FR-062).
/// A campaign that found nothing and a campaign that never ran are completely different
/// facts, and without the volume they are indistinguishable from the outside — which would
/// make "no findings" the most comfortable possible way for the machinery to be broken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignOutcomeReport {
    /// `cmp-<hash8>`.
    pub campaign: String,
    /// The recorded seed — the reproducibility input (FR-001).
    pub seed: String,
    /// The tier's wire spelling.
    pub tier: String,
    /// The lane's wire spelling.
    pub lane: String,
    /// The certification profile the run happened under.
    pub profile: String,
    /// All seven pinned inputs, verbatim (FR-002).
    pub pinned_input_set: PinnedInputSet,
    /// The declared budget.
    pub budget: Budget,
    /// Candidates the generator produced.
    pub candidates_generated: u64,
    /// Candidates actually executed against an implementation.
    pub candidates_executed: u64,
    /// Candidates discarded as unsafe before execution (FR-011).
    pub candidates_discarded_unsafe: u64,
    /// Candidates that failed at document parsing.
    pub parse_stage_failures: u64,
    /// `parseStageFailures / candidatesGenerated` — the SC-002 ratio, reported for **every**
    /// run rather than only when it breaches, so a rising trend is visible before it does.
    ///
    /// Zero when nothing was generated: a campaign that produced no candidates failed no
    /// candidates either, and reporting a ratio of "not a number" would serialize as bare
    /// `null` and never load back.
    pub trivial_failure_fraction: f64,
    /// The declared SC-002 ceiling this run is judged against.
    pub trivial_failure_ceiling: f64,
    /// Whether the run breached the ceiling. **Reported, never gating** — the exit status
    /// of a discovery command reflects whether it ran, never what it found.
    pub trivial_failure_ceiling_breached: bool,
    /// Whether the wall-clock budget ran out (FR-005).
    pub budget_exhausted: bool,
    /// The fraction of the planned space covered, reported when the budget was exhausted
    /// (FR-005) so a truncated run is never presented as complete.
    pub space_covered_fraction: f64,
    /// Applications per mutation category — **all eleven keys, always** (FR-010).
    pub mutation_applications: IndexMap<String, u64>,
    /// Categories with zero successful applications, named as an explicit generation
    /// deficiency (FR-010, SC-003) rather than left to be inferred from a map a reader
    /// would have to cross-check against the catalogue.
    pub unapplied_categories: Vec<String>,
    /// Distinct signatures observed.
    pub signatures_observed: u64,
    /// Distinct signatures admitted to the queue.
    pub signatures_admitted: u64,
    /// Distinct signatures the admission cap suppressed (FR-034b) — never silent.
    pub signatures_suppressed: u64,
    /// The findings this campaign admitted or re-witnessed, in admission order.
    pub findings: Vec<String>,
}

/// The SC-002 ceiling: at most 10% of generated candidates may fail at the
/// document-syntax stage. Above it, the campaign explored the parser rather than the tool.
pub const TRIVIAL_FAILURE_CEILING: f64 = 0.10;

/// Re-key an application map onto the catalogue's eleven declared categories.
///
/// Starts from [`mutate::empty_application_counts`] — the single source of the key list —
/// so the result carries every category whatever the caller recorded, and a category the
/// caller invented is dropped rather than silently widening the catalogue. FR-010 needs
/// the keys present to distinguish "never applied" from "never mentioned"; it does not
/// need a twelfth key nothing declares.
pub fn normalized_mutation_applications(recorded: &IndexMap<String, u64>) -> IndexMap<String, u64> {
    let mut counts = mutate::empty_application_counts();
    for (category, count) in recorded {
        if let Some(slot) = counts.get_mut(category) {
            *slot = *count;
        }
    }
    counts
}

/// Build a campaign's own report. Pure: no I/O, no clock, no environment.
pub fn build_campaign_outcome_report(
    campaign: &Campaign,
    findings: &[String],
) -> CampaignOutcomeReport {
    let outcome = &campaign.outcome;
    let mutation_applications = normalized_mutation_applications(&outcome.mutation_applications);
    let unapplied: Vec<String> = mutate::unapplied_categories(&mutation_applications)
        .into_iter()
        .map(str::to_string)
        .collect();
    // A ratio over a zero denominator is `NaN`, which `serde_json` renders as bare `null`
    // and cannot read back — the same hazard `write_campaigns` refuses for
    // `spaceCoveredFraction`. Zero candidates means zero trivial failures, so zero is the
    // honest answer rather than a fallback.
    let trivial_failure_fraction = if outcome.candidates_generated == 0 {
        0.0
    } else {
        outcome.parse_stage_failures as f64 / outcome.candidates_generated as f64
    };

    CampaignOutcomeReport {
        campaign: campaign.id.clone(),
        seed: campaign.seed.clone(),
        tier: campaign.tier.as_str().to_string(),
        lane: campaign.lane.as_str().to_string(),
        profile: campaign.profile.clone(),
        pinned_input_set: campaign.pinned_input_set.clone(),
        budget: campaign.budget,
        candidates_generated: outcome.candidates_generated,
        candidates_executed: outcome.candidates_executed,
        candidates_discarded_unsafe: outcome.candidates_discarded_unsafe,
        parse_stage_failures: outcome.parse_stage_failures,
        trivial_failure_fraction,
        trivial_failure_ceiling: TRIVIAL_FAILURE_CEILING,
        trivial_failure_ceiling_breached: trivial_failure_fraction > TRIVIAL_FAILURE_CEILING,
        budget_exhausted: outcome.budget_exhausted,
        space_covered_fraction: outcome.space_covered_fraction,
        mutation_applications,
        unapplied_categories: unapplied,
        signatures_observed: outcome.signatures_observed,
        signatures_admitted: outcome.signatures_admitted,
        signatures_suppressed: outcome.signatures_suppressed,
        findings: findings.to_vec(),
    }
}

/// Render the campaign report as the byte-stable JSON document the bin prints on stdout.
pub fn render_campaign_json(report: &CampaignOutcomeReport) -> String {
    let mut out = serde_json::to_string_pretty(report)
        .unwrap_or_else(|e| unreachable!("campaign-report serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Render the campaign report for human review.
pub fn render_campaign_md(report: &CampaignOutcomeReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _ = writeln!(out, "# Discovery campaign `{}`\n", report.campaign);
    let _ = writeln!(
        out,
        "Findings are **candidates for assertions**, never assertions. This report never \
         gates: its existence says the campaign ran, and its contents say what it saw.\n"
    );

    let _ = writeln!(out, "## Reproducing this run\n");
    let _ = writeln!(out, "| Input | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| seed | `{}` |", report.seed);
    let _ = writeln!(out, "| tier | {} |", report.tier);
    let _ = writeln!(out, "| lane | {} |", report.lane);
    let _ = writeln!(out, "| profile | `{}` |", report.profile);
    let pins = &report.pinned_input_set;
    for (name, value) in [
        ("schemaPin", &pins.schema_pin),
        ("prosePin", &pins.prose_pin),
        ("oracleVersion", &pins.oracle_version),
        ("normalizerVersion", &pins.normalizer_version),
        ("grammarVersion", &pins.grammar_version),
        ("mutationCatalogVersion", &pins.mutation_catalog_version),
        ("generatorVersion", &pins.generator_version),
    ] {
        let _ = writeln!(out, "| {name} | `{value}` |");
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Volume\n");
    let _ = writeln!(
        out,
        "Reported whether or not anything was found (FR-062): a campaign that found \
         nothing and a campaign that never ran are different facts.\n"
    );
    let _ = writeln!(out, "| Measure | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(
        out,
        "| candidatesGenerated | {} |",
        report.candidates_generated
    );
    let _ = writeln!(
        out,
        "| candidatesExecuted | {} |",
        report.candidates_executed
    );
    let _ = writeln!(
        out,
        "| candidatesDiscardedUnsafe | {} |",
        report.candidates_discarded_unsafe
    );
    let _ = writeln!(
        out,
        "| parseStageFailures | {} |",
        report.parse_stage_failures
    );
    let _ = writeln!(
        out,
        "| trivialFailureFraction | {:.4} (ceiling {:.2}{}) |",
        report.trivial_failure_fraction,
        report.trivial_failure_ceiling,
        if report.trivial_failure_ceiling_breached {
            ", **BREACHED**"
        } else {
            ""
        }
    );
    let _ = writeln!(out, "| budgetExhausted | {} |", report.budget_exhausted);
    let _ = writeln!(
        out,
        "| spaceCoveredFraction | {:.4} |",
        report.space_covered_fraction
    );
    let _ = writeln!(
        out,
        "| signaturesObserved | {} |",
        report.signatures_observed
    );
    let _ = writeln!(
        out,
        "| signaturesAdmitted | {} |",
        report.signatures_admitted
    );
    let _ = writeln!(
        out,
        "| signaturesSuppressed | {} |\n",
        report.signatures_suppressed
    );

    let _ = writeln!(out, "## Mutation applications\n");
    let _ = writeln!(
        out,
        "All {} declared categories are listed, including zeroes: a category absent from \
         the table would be indistinguishable from one that was never applied (FR-010).\n",
        report.mutation_applications.len()
    );
    let _ = writeln!(out, "| Category | Applications |");
    let _ = writeln!(out, "|---|---|");
    for (category, count) in &report.mutation_applications {
        let _ = writeln!(out, "| `{category}` | {count} |");
    }
    let _ = writeln!(out);
    if report.unapplied_categories.is_empty() {
        let _ = writeln!(out, "Every declared category was applied at least once.\n");
    } else {
        let _ = writeln!(
            out,
            "**Generation deficiency**: {} categor{} never applied — {}. A category with \
             zero successful applications is a hole in what this campaign explored, not a \
             detail of its bookkeeping.\n",
            report.unapplied_categories.len(),
            if report.unapplied_categories.len() == 1 {
                "y"
            } else {
                "ies"
            },
            report
                .unapplied_categories
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    let _ = writeln!(out, "## Findings ({})\n", report.findings.len());
    if report.findings.is_empty() {
        let _ = writeln!(
            out,
            "This campaign admitted no findings. Read that against the volume above: it \
             says the two implementations agreed on everything this run reached, not that \
             nothing ran.\n"
        );
    } else {
        for finding in &report.findings {
            let _ = writeln!(out, "- `{finding}`");
        }
        let _ = writeln!(out);
    }

    out
}

/// Atomically write `campaign-<id>.{json,md}` into `dir`, returning the written paths in a
/// deterministic order.
pub fn write_campaign_outcome_report(
    dir: &Path,
    report: &CampaignOutcomeReport,
) -> std::io::Result<Vec<PathBuf>> {
    let json = dir.join(format!("campaign-{}.json", report.campaign));
    let md = dir.join(format!("campaign-{}.md", report.campaign));
    crate::atomic_write(&json, &render_campaign_json(report))?;
    crate::atomic_write(&md, &render_campaign_md(report))?;
    Ok(vec![json, md])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::queue::{
        Budget, Campaign, CampaignLane, CampaignOutcome, CampaignTier, Classification, Finding,
        ObservedValues, PinnedInputSet, Witness,
    };
    use crate::discovery::signature::{Divergence, DivergenceKind, Signature};
    use indexmap::IndexMap;
    use serde_json::json;

    fn pins() -> CurrentPins {
        CurrentPins {
            schema_pin: "113500f4".into(),
            prose_pin: "113500f4".into(),
            oracle_version: Some("0.87.0".into()),
        }
    }

    fn campaign(id: &str, oracle: &str) -> Campaign {
        Campaign {
            id: id.into(),
            seed: "0x5eed1234".into(),
            lane: CampaignLane::Scheduled,
            tier: CampaignTier::ConfigDifferential,
            // The report never validates identity — `check` owns D1 — so these fixtures
            // keep readable ids rather than derived ones. A report that refused to render
            // a structurally invalid queue would hide exactly the queue a reviewer most
            // needs to see.
            profile: "prof-linux-amd64-docker-0870".into(),
            pinned_input_set: PinnedInputSet {
                schema_pin: "113500f4".into(),
                prose_pin: "113500f4".into(),
                oracle_version: oracle.into(),
                normalizer_version: "6".into(),
                grammar_version: "rev-schema-113500f4".into(),
                mutation_catalog_version: "v1".into(),
                generator_version: "splitmix64-seed+xoshiro256starstar/v1".into(),
            },
            budget: Budget {
                wall_clock_seconds: 1800,
                per_candidate_seconds: 60,
                shrink_steps_per_finding: 64,
                admission_cap: 25,
            },
            outcome: CampaignOutcome {
                candidates_generated: 4820,
                candidates_executed: 4629,
                candidates_discarded_unsafe: 0,
                parse_stage_failures: 191,
                budget_exhausted: false,
                space_covered_fraction: 0.0,
                mutation_applications: IndexMap::new(),
                signatures_observed: 2,
                signatures_admitted: 2,
                signatures_suppressed: 0,
            },
        }
    }

    fn finding(path: &str, state: FindingState, campaign_id: &str) -> Finding {
        let d = json!("vscode");
        let r = json!("root");
        let sig = Signature::derive(
            "chan-structured-output",
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: Some(&d),
                reference: Some(&r),
            },
        );
        let witness = Witness {
            id: Witness::derived_id(campaign_id, "cnd-1"),
            campaign_id: campaign_id.into(),
            candidate_id: "cnd-1".into(),
            minimal_input: json!({ "image": "alpine:3.18" }),
            is_minimal: true,
            reduction_steps: Vec::new(),
            observed_values: ObservedValues::default(),
            mutation_operators: Vec::new(),
        };
        let mut f = Finding::newly_admitted(sig, witness, campaign_id);
        f.state = state;
        if state != FindingState::Untriaged {
            f.classification = Some(Classification::DeaconRegression);
        }
        f
    }

    #[test]
    fn an_empty_queue_reports_zeroes_and_says_what_that_does_not_mean() {
        let report = build_queue_report(&DiscoveryData::default(), &pins());
        assert_eq!(report.total, 0);
        assert!(report.untriaged.is_empty());
        assert!(report.campaigns.is_empty());

        let md = render_md(&report);
        assert!(md.contains("| untriaged | 0 |"));
        assert!(
            md.contains("nothing ran"),
            "an empty queue must not read as agreement between the implementations"
        );

        let json = render_json(&report);
        let back: QueueReport = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, report);
    }

    #[test]
    fn findings_land_in_their_state_buckets() {
        let data = DiscoveryData {
            findings: vec![
                finding("a", FindingState::Untriaged, "cmp-1"),
                finding("b", FindingState::Triaged, "cmp-1"),
                finding("c", FindingState::Split, "cmp-1"),
                finding("d", FindingState::Promoted, "cmp-1"),
                finding("e", FindingState::NoLongerReproducing, "cmp-1"),
            ],
            campaigns: vec![campaign("cmp-1", "0.87.0")],
            corpus: Vec::new(),
        };
        let report = build_queue_report(&data, &pins());
        assert_eq!(report.total, 5);
        assert_eq!(report.untriaged.len(), 1);
        assert_eq!(report.triaged.len(), 1);
        assert_eq!(report.split.len(), 1);
        assert_eq!(report.promoted.len(), 1);
        assert_eq!(report.no_longer_reproducing.len(), 1);
        assert!(report.pin_stale.is_empty());

        let md = render_md(&report);
        assert!(md.contains("| untriaged | 1 |"));
        assert!(md.contains("Nobody has looked yet"));
    }

    #[test]
    fn a_finding_observed_under_old_pins_is_pin_stale_but_keeps_its_state() {
        let data = DiscoveryData {
            findings: vec![finding("a", FindingState::Triaged, "cmp-old")],
            campaigns: vec![campaign("cmp-old", "0.86.0")],
            corpus: Vec::new(),
        };
        let report = build_queue_report(&data, &pins());
        assert_eq!(report.pin_stale.len(), 1);
        assert_eq!(
            report.triaged.len(),
            1,
            "pin staleness is cross-cutting: a pin moving must not discard a reviewer's \
             judgement"
        );
    }

    #[test]
    fn an_undecidable_oracle_pin_does_not_mark_the_whole_queue_stale() {
        // A registry that has not recorded an oracle revision means "we cannot tell",
        // which must never render as "everything differs" — that would read as a
        // catastrophic pin bump instead of as missing metadata.
        let undecidable = CurrentPins {
            oracle_version: None,
            ..pins()
        };
        let data = DiscoveryData {
            findings: vec![finding("a", FindingState::Triaged, "cmp-1")],
            campaigns: vec![campaign("cmp-1", "0.87.0")],
            corpus: Vec::new(),
        };
        assert!(build_queue_report(&data, &undecidable).pin_stale.is_empty());

        // The other two elements are still compared, so a real schema-pin bump is caught
        // even while the oracle pin is undecidable.
        let bumped = CurrentPins {
            schema_pin: "deadbeef".into(),
            ..undecidable
        };
        assert_eq!(build_queue_report(&data, &bumped).pin_stale.len(), 1);
    }

    #[test]
    fn campaign_volume_is_reported_even_with_no_findings() {
        let data = DiscoveryData {
            findings: Vec::new(),
            campaigns: vec![campaign("cmp-1", "0.87.0")],
            corpus: Vec::new(),
        };
        let report = build_queue_report(&data, &pins());
        assert_eq!(report.total, 0);
        assert_eq!(report.campaigns[0].candidates_generated, 4820);
        let md = render_md(&report);
        assert!(
            md.contains("4820"),
            "nothing-found must not read as nothing-ran"
        );
    }

    #[test]
    fn the_rendering_is_byte_stable_and_writes_atomically() {
        let data = DiscoveryData {
            findings: vec![finding("a", FindingState::Untriaged, "cmp-1")],
            campaigns: vec![campaign("cmp-1", "0.87.0")],
            corpus: Vec::new(),
        };
        let report = build_queue_report(&data, &pins());
        assert_eq!(render_json(&report), render_json(&report));
        assert_eq!(render_md(&report), render_md(&report));

        let dir = tempfile::tempdir().expect("tempdir");
        let written = write_queue_report(dir.path(), &report).expect("writes");
        assert_eq!(written.len(), 2);
        assert!(written[0].ends_with("queue.json"));
        assert!(written[1].ends_with("queue.md"));
        let json = std::fs::read_to_string(&written[0]).expect("read");
        assert_eq!(json, render_json(&report));
        assert!(json.ends_with("}\n"), "trailing newline for a clean diff");
    }

    // --- T039 / T123: the campaign-outcome report -------------------------

    #[test]
    fn the_campaign_report_always_carries_all_eleven_mutation_keys() {
        // FR-010: a category absent from the map is indistinguishable from a category
        // that was never applied. A campaign that recorded only the categories that fired
        // is exactly the shape this normalization exists to repair.
        let mut c = campaign("cmp-1", "0.87.0");
        c.outcome.mutation_applications = IndexMap::from([("wrong-type".to_string(), 12u64)]);

        let report = build_campaign_outcome_report(&c, &[]);
        assert_eq!(report.mutation_applications.len(), 11);
        let keys: Vec<&str> = report
            .mutation_applications
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            crate::discovery::mutate::category_names(),
            "the key list is the catalogue's, in declaration order — never restated here"
        );
        assert_eq!(report.mutation_applications["wrong-type"], 12);
        assert_eq!(report.mutation_applications["ordering-change"], 0);
        assert_eq!(
            report.unapplied_categories.len(),
            10,
            "every zero-count category is named as an explicit generation deficiency"
        );
        assert!(
            report
                .unapplied_categories
                .contains(&"null-value".to_string())
        );

        let md = render_campaign_md(&report);
        for category in crate::discovery::mutate::category_names() {
            assert!(
                md.contains(&format!("`{category}`")),
                "{category} missing from {md}"
            );
        }
        assert!(md.contains("Generation deficiency"));
    }

    #[test]
    fn a_category_the_catalogue_does_not_declare_is_not_admitted_by_the_report() {
        // The map's job is to make a missing category visible, not to widen the catalogue:
        // an invented key would report a generation category nothing can ever apply.
        let mut c = campaign("cmp-1", "0.87.0");
        c.outcome.mutation_applications =
            IndexMap::from([("invented-category".to_string(), 99u64)]);
        let report = build_campaign_outcome_report(&c, &[]);
        assert_eq!(report.mutation_applications.len(), 11);
        assert!(
            !report
                .mutation_applications
                .contains_key("invented-category")
        );
        assert_eq!(report.unapplied_categories.len(), 11);
    }

    #[test]
    fn a_full_catalogue_reports_no_deficiency() {
        let mut c = campaign("cmp-1", "0.87.0");
        let mut counts = crate::discovery::mutate::empty_application_counts();
        for (i, name) in crate::discovery::mutate::category_names()
            .iter()
            .enumerate()
        {
            counts.insert((*name).to_string(), i as u64 + 1);
        }
        c.outcome.mutation_applications = counts;
        let report = build_campaign_outcome_report(&c, &[]);
        assert!(report.unapplied_categories.is_empty());
        assert!(render_campaign_md(&report).contains("Every declared category was applied"));
    }

    #[test]
    fn a_zero_finding_campaign_still_reports_the_volume_it_covered() {
        // T123 / FR-062: "nothing found" must be distinguishable from "nothing ran".
        // Without the volume, a broken pipeline is the most comfortable possible state for
        // the machinery to be in — it reports the same thing as a clean run.
        let c = campaign("cmp-1", "0.87.0");
        let report = build_campaign_outcome_report(&c, &[]);
        assert!(report.findings.is_empty());
        assert_eq!(report.candidates_generated, 4820);
        assert_eq!(report.candidates_executed, 4629);

        let json = render_campaign_json(&report);
        assert!(json.contains("\"candidatesGenerated\": 4820"));
        assert!(json.contains("\"candidatesExecuted\": 4629"));
        let back: CampaignOutcomeReport = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, report);

        let md = render_campaign_md(&report);
        assert!(md.contains("4820"));
        assert!(md.contains("4629"));
        assert!(
            md.contains("not that \nnothing ran") || md.contains("not that nothing ran"),
            "the empty-findings note must say what the absence does NOT mean: {md}"
        );
    }

    #[test]
    fn a_campaign_that_ran_nothing_reports_zeroes_rather_than_a_not_a_number_ratio() {
        // `NaN` serializes as bare `null` and never loads back — the same hazard
        // `write_campaigns` refuses for `spaceCoveredFraction`. An aborted campaign is
        // exactly the zero-denominator shape.
        let mut c = campaign("cmp-1", "0.87.0");
        c.outcome.candidates_generated = 0;
        c.outcome.candidates_executed = 0;
        c.outcome.parse_stage_failures = 0;

        let report = build_campaign_outcome_report(&c, &[]);
        assert_eq!(report.trivial_failure_fraction, 0.0);
        assert!(report.trivial_failure_fraction.is_finite());
        assert!(!report.trivial_failure_ceiling_breached);
        let json = render_campaign_json(&report);
        let raw: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        for field in ["trivialFailureFraction", "spaceCoveredFraction"] {
            assert!(
                raw[field].is_number(),
                "`{field}` rendered as {} — a non-finite f64 serializes as bare `null` and \
                 never loads back",
                raw[field]
            );
        }
        serde_json::from_str::<CampaignOutcomeReport>(&json).expect("loads back");
    }

    #[test]
    fn the_trivial_failure_ratio_is_reported_for_every_run_and_never_gates() {
        let mut c = campaign("cmp-1", "0.87.0");
        c.outcome.candidates_generated = 100;
        c.outcome.parse_stage_failures = 4;
        let ok = build_campaign_outcome_report(&c, &[]);
        assert!((ok.trivial_failure_fraction - 0.04).abs() < 1e-9);
        assert!(!ok.trivial_failure_ceiling_breached);

        c.outcome.parse_stage_failures = 40;
        let breached = build_campaign_outcome_report(&c, &[]);
        assert!(breached.trivial_failure_ceiling_breached);
        assert!(render_campaign_md(&breached).contains("BREACHED"));
        // Reporting the breach is the whole response: the report has no exit status of its
        // own, and the bin's status reflects whether the campaign ran (FR-058).
        assert_eq!(breached.trivial_failure_ceiling, TRIVIAL_FAILURE_CEILING);
    }

    #[test]
    fn the_campaign_report_is_byte_stable_and_writes_atomically() {
        let c = campaign("cmp-1", "0.87.0");
        let report = build_campaign_outcome_report(&c, &["fnd-11111111".to_string()]);
        assert_eq!(render_campaign_json(&report), render_campaign_json(&report));
        assert_eq!(render_campaign_md(&report), render_campaign_md(&report));

        let dir = tempfile::tempdir().expect("tempdir");
        let written = write_campaign_outcome_report(dir.path(), &report).expect("writes");
        assert_eq!(written.len(), 2);
        assert!(written[0].ends_with("campaign-cmp-1.json"));
        assert!(written[1].ends_with("campaign-cmp-1.md"));
        assert_eq!(
            std::fs::read_to_string(&written[0]).expect("read"),
            render_campaign_json(&report)
        );
        assert!(
            std::fs::read_to_string(&written[1])
                .expect("read")
                .contains("fnd-11111111")
        );
    }

    #[test]
    fn the_campaign_report_states_every_pinned_input_and_the_seed() {
        // FR-061: a run's report must state what would be needed to reproduce it. A report
        // that omits one of the seven pins describes a run nobody can re-run.
        let c = campaign("cmp-1", "0.87.0");
        let md = render_campaign_md(&build_campaign_outcome_report(&c, &[]));
        for expected in [
            "0x5eed1234",
            "schemaPin",
            "prosePin",
            "oracleVersion",
            "normalizerVersion",
            "grammarVersion",
            "mutationCatalogVersion",
            "generatorVersion",
            "prof-linux-amd64-docker-0870",
        ] {
            assert!(md.contains(expected), "{expected} missing from the report");
        }
    }

    #[test]
    fn a_dangling_last_observed_does_not_mark_a_finding_stale() {
        // An unresolvable campaign reference is D1's problem. The report must not turn a
        // structural violation into a silently different classification.
        let data = DiscoveryData {
            findings: vec![finding("a", FindingState::Triaged, "cmp-ghost")],
            campaigns: Vec::new(),
            corpus: Vec::new(),
        };
        let report = build_queue_report(&data, &pins());
        assert!(report.pin_stale.is_empty());
        assert_eq!(report.triaged.len(), 1);
    }
}
