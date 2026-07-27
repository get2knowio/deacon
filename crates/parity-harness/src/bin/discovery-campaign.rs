//! `discovery-campaign` — run one exploratory parity discovery campaign
//! (025-exploratory-parity-discovery, T038; contracts/discovery-cli.md § Live commands).
//!
//! ```text
//! cargo run -p parity-harness --bin discovery-campaign -- \
//!     --seed <hex> --tier <t> [--budget-seconds <n>] [--lane <l>] [--candidates <n>] \
//!     [--profile <prof-id>] [--dry-run]
//! ```
//!
//! # `--seed` is required, never defaulted
//!
//! A defaulted seed would let a campaign run without its reproducibility input being a
//! conscious choice, and FR-001 depends on the seed being *recorded* rather than inferred.
//! A default would also be the worst kind of silent: every unattended run would share one
//! stream, so the campaign would explore the same neighbourhood forever while reporting
//! volume as if it were coverage.
//!
//! # Exit status reflects whether it ran, never what it found (FR-058)
//!
//! | Status | Meaning |
//! |---|---|
//! | `0` | the campaign ran to completion or to budget exhaustion — **regardless of findings** |
//! | `1` | a prerequisite failed, normalization failed, or the data root was unwritable |
//! | `2` | the invocation itself was malformed (a usage error) |
//!
//! A campaign that finds forty differences exits `0`. Any command whose status depends on
//! its findings becomes a gate the moment someone wires it into CI, and a stochastic gate
//! makes green non-reproducible — which is exactly what would make the discovery lane
//! unsafe to schedule.
//!
//! # What it writes, and what it must never write
//!
//! Writes `conformance/discovery/{findings,campaigns}.json` (atomically) and raw evidence
//! under `target/discovery/`. It **never** writes anything under `conformance/registry/`,
//! `conformance/snapshots/`, or `conformance/obligations/` (FR-036): a stochastic process
//! must not be able to author the record it is tested against.

use std::process::ExitCode;

use deacon_conformance::discovery::queue::{
    Budget, CampaignLane, CampaignTier, DEFAULT_ADMISSION_CAP,
    DEFAULT_PER_CANDIDATE_SECONDS_CONTAINER, DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC,
    DEFAULT_WALL_CLOCK_SECONDS,
};
use deacon_conformance::discovery::report::render_campaign_json;
use deacon_conformance::{default_discovery_dir, default_registry_dir, workspace_root};

use parity_harness::discovery::campaign::{self, CampaignRequest};
use parity_harness::prereq::deacon_binary;

/// The default certification profile a campaign records itself under when none is given.
///
/// Named rather than derived: the profile is a claim about the environment the run
/// happened in, and guessing it from the host would make two machines silently disagree
/// about what a finding is a claim about.
const DEFAULT_PROFILE: &str = "prof-linux-amd64-docker-0870";

/// How many candidates a campaign plans to reach when the caller does not say.
///
/// The denominator of `spaceCoveredFraction`: on exhaustion the run reports the portion of
/// *this plan* it covered rather than presenting a truncated run as complete (FR-005).
const DEFAULT_PLANNED_CANDIDATES: u64 = 200;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse(&args) {
        Ok(p) => p,
        Err(message) => return usage(&message),
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: could not start async runtime: {e}");
            return ExitCode::from(1);
        }
    };

    let deacon = match runtime.block_on(deacon_binary()) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    let request = CampaignRequest {
        seed_hex: parsed.seed_hex.clone(),
        seed: parsed.seed,
        tier: parsed.tier,
        lane: parsed.lane,
        profile: parsed.profile.clone(),
        budget: Budget {
            wall_clock_seconds: parsed.budget_seconds,
            per_candidate_seconds: match parsed.tier {
                CampaignTier::ContainerDifferential => DEFAULT_PER_CANDIDATE_SECONDS_CONTAINER,
                _ => DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC,
            },
            shrink_steps_per_finding: 64,
            admission_cap: DEFAULT_ADMISSION_CAP,
        },
        planned_candidates: parsed.planned_candidates,
        registry_dir: default_registry_dir(),
        discovery_dir: default_discovery_dir(),
        report_root: workspace_root().join("target").join("discovery"),
        deacon_binary: deacon,
        oracle_override: None,
        persist: !parsed.dry_run,
    };

    match runtime.block_on(campaign::run(&request)) {
        Ok(run) => {
            // stdout: a single JSON campaign outcome. stderr: everything else — so a
            // caller can pipe the outcome into `jq` without the diagnostics landing in it.
            print!("{}", render_campaign_json(&run.report));
            eprintln!(
                "campaign {} ran: {} generated, {} executed, {} discarded, {} finding(s) \
                 admitted, {} suppressed, {} already characterized",
                run.campaign.id,
                run.report.candidates_generated,
                run.report.candidates_executed,
                run.report.candidates_discarded_unsafe,
                run.report.signatures_admitted,
                run.report.signatures_suppressed,
                run.characterized_observations,
            );
            // The corpus tier's per-entry outcomes, named one by one (FR-051/FR-052). The
            // aggregate counters cannot carry this: "33 generated, 31 executed" says two
            // entries did not run, and an ecological canary whose whole job is to notice
            // the ecosystem moving must say WHICH two and whether the reason was an
            // unreachable snapshot or content that stopped matching its recorded digest.
            if !run.corpus_statuses.is_empty() {
                eprintln!("corpus entries ({}):", run.corpus_statuses.len());
                for status in &run.corpus_statuses {
                    eprintln!("  {}", status.summary());
                }
            }
            // Only the *generating* tiers can have a generation deficiency. The metamorphic
            // and corpus tiers draw nothing and mutate nothing — their inputs are
            // hand-authored relations and pinned third-party snapshots — so all eleven
            // categories are legitimately zero, and reporting that as a deficiency would
            // teach a reader to ignore the one line that matters when a real generator
            // regression makes a category stop firing.
            let generates = matches!(
                parsed.tier,
                CampaignTier::ConfigDifferential | CampaignTier::ContainerDifferential
            );
            if generates && !run.report.unapplied_categories.is_empty() {
                eprintln!(
                    "generation deficiency: {} mutation categor(y|ies) never applied — {}",
                    run.report.unapplied_categories.len(),
                    run.report.unapplied_categories.join(", ")
                );
            }
            // Exit ZERO whatever was found. This is the whole exit-status contract.
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// The parsed invocation.
struct Parsed {
    seed_hex: String,
    seed: u64,
    tier: CampaignTier,
    lane: CampaignLane,
    profile: String,
    budget_seconds: u64,
    planned_candidates: u64,
    dry_run: bool,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut seed_hex: Option<String> = None;
    let mut tier: Option<CampaignTier> = None;
    let mut lane = CampaignLane::Invoked;
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut budget_seconds = DEFAULT_WALL_CLOCK_SECONDS;
    let mut planned_candidates = DEFAULT_PLANNED_CANDIDATES;
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let mut value = || -> Result<String, String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--seed" => seed_hex = Some(value()?),
            "--tier" => {
                let raw = value()?;
                tier = Some(
                    CampaignTier::all()
                        .iter()
                        .copied()
                        .find(|t| t.as_str() == raw)
                        .ok_or_else(|| {
                            format!(
                                "unknown tier {raw:?}; expected one of {}",
                                CampaignTier::all()
                                    .iter()
                                    .map(|t| t.as_str())
                                    .collect::<Vec<&str>>()
                                    .join(", ")
                            )
                        })?,
                );
            }
            "--lane" => {
                let raw = value()?;
                lane = match raw.as_str() {
                    "scheduled" => CampaignLane::Scheduled,
                    "invoked" => CampaignLane::Invoked,
                    other => {
                        return Err(format!(
                            "unknown lane {other:?}; expected `scheduled` or `invoked`"
                        ));
                    }
                };
            }
            "--profile" => profile = value()?,
            "--budget-seconds" => {
                let raw = value()?;
                budget_seconds = raw
                    .parse()
                    .map_err(|e| format!("--budget-seconds {raw:?} is not a number: {e}"))?;
            }
            "--candidates" => {
                let raw = value()?;
                planned_candidates = raw
                    .parse()
                    .map_err(|e| format!("--candidates {raw:?} is not a number: {e}"))?;
            }
            "--dry-run" => dry_run = true,
            other => return Err(format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    // Required, never defaulted (FR-001).
    let seed_hex = seed_hex.ok_or_else(|| {
        "--seed is required and is never defaulted: a campaign must not run without its \
         reproducibility input being a conscious, recorded choice (FR-001)"
            .to_string()
    })?;
    let seed = parse_seed(&seed_hex)?;
    let tier = tier.ok_or_else(|| {
        format!(
            "--tier is required; expected one of {}",
            CampaignTier::all()
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<&str>>()
                .join(", ")
        )
    })?;
    if planned_candidates == 0 {
        return Err(
            "--candidates must be at least 1: a campaign that plans nothing covers \
                    nothing, and reporting a fraction of zero would divide by it"
                .to_string(),
        );
    }

    Ok(Parsed {
        seed_hex,
        seed,
        tier,
        lane,
        profile,
        budget_seconds,
        planned_candidates,
        dry_run,
    })
}

/// Parse a seed, accepting `0x`-prefixed hex or plain hex.
///
/// Rejected rather than hashed into a `u64` on failure: a seed that silently became some
/// other number would record a value that does not reproduce the run it names, which is
/// worse than refusing the invocation.
fn parse_seed(raw: &str) -> Result<u64, String> {
    let trimmed = raw.trim();
    let digits = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    u64::from_str_radix(digits, 16).map_err(|e| {
        format!(
            "--seed {raw:?} is not a 64-bit hex value: {e}. The seed is recorded \
                 verbatim and must reproduce the run it names."
        )
    })
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    eprintln!(
        "usage: discovery-campaign --seed <hex> --tier <{}> \
         [--lane scheduled|invoked] [--profile <prof-id>] [--budget-seconds <n>] \
         [--candidates <n>] [--dry-run]",
        CampaignTier::all()
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<&str>>()
            .join("|")
    );
    ExitCode::from(2)
}
