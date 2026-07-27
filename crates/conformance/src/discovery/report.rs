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
//! The campaign-level suppression detail and the per-behavior grouping view are extended
//! by **T039**/**T073**; the five buckets below are the contract's own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::queue::{Campaign, DiscoveryData, Finding, FindingState};

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
    /// The campaign history, in file order.
    pub campaigns: Vec<CampaignSummary>,
}

/// Build the report. Pure: no I/O, no clock, no environment.
pub fn build_queue_report(data: &DiscoveryData, pins: &CurrentPins) -> QueueReport {
    let mut report = QueueReport {
        total: data.findings.len(),
        campaigns: data.campaigns.iter().map(CampaignSummary::of).collect(),
        ..QueueReport::default()
    };

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
    let _ = writeln!(out, "| campaigns | {} |\n", report.campaigns.len());

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

    #[test]
    fn a_dangling_last_observed_does_not_mark_a_finding_stale() {
        // An unresolvable campaign reference is D1's problem. The report must not turn a
        // structural violation into a silently different classification.
        let data = DiscoveryData {
            findings: vec![finding("a", FindingState::Triaged, "cmp-ghost")],
            campaigns: Vec::new(),
        };
        let report = build_queue_report(&data, &pins());
        assert!(report.pin_stale.is_empty());
        assert_eq!(report.triaged.len(), 1);
    }
}
