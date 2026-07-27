//! `discovery-proof` — prove the discovery pipeline can carry an injected difference end
//! to end (025-exploratory-parity-discovery, T082; contracts/discovery-cli.md § Live
//! commands).
//!
//! ```text
//! cargo run -p parity-harness --bin discovery-proof -- \
//!     [--seed <hex>] [--profile <prof-id>] [--candidates <n>] [--shrink-budget <n>] \
//!     [--out <file>]
//! ```
//!
//! # The one discovery command whose status depends on an outcome
//!
//! Every other discovery command's exit status reflects **whether it ran**, never **what it
//! found** (FR-058) — a campaign that surfaces forty differences exits `0`, because a
//! stochastic gate makes green non-reproducible. This command is the exception, and it is
//! not really an exception: its status is not finding-dependent. It asserts a property of
//! the **machinery**, so non-zero means the pipeline is broken, which is exactly the thing
//! that should fail a lane.
//!
//! | Status | Meaning |
//! |---|---|
//! | `0` | every injected difference traversed all six stages |
//! | `1` | an injection landed and failed to surface (**pipeline defect**), an injection never landed (**proof defect**), or a prerequisite failed |
//! | `2` | the invocation itself was malformed (a usage error) |
//!
//! An injection that never landed exits `1` as `InjectionInapplicable` **rather than being
//! counted as "found nothing"**. Those are opposite conclusions, and a proof that merged
//! them would be the most comfortable possible way for this feature to be broken.
//!
//! # Prerequisites: deacon, and nothing else
//!
//! No oracle, no Docker, no network. The proof compares deacon against its own unperturbed
//! run with a known difference injected into one side at the sealed evidence-source
//! boundary — see the module docs of
//! [`pipeline_proof`](parity_harness::discovery::pipeline_proof) for why a reference takes
//! no part in whether the machinery propagates a difference, and why the self-comparison is
//! what makes the baseline provably clean.
//!
//! # What it writes
//!
//! `target/discovery/proof.json` plus reviewable candidates under
//! `target/discovery/proof/candidates/`. It writes **nothing** under
//! `conformance/registry/`, `conformance/discovery/`, `conformance/snapshots/`, or
//! `conformance/obligations/` (FR-036): a stochastic process must not be able to author the
//! record it is tested against, and the promotion stage passes by showing that gate holding
//! rather than by walking through it.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use deacon_conformance::{default_registry_dir, workspace_root};

use parity_harness::discovery::pipeline_proof::{self, ProofRequest, TraversalVerdict};
use parity_harness::inject::RegressionHarness;
use parity_harness::prereq::deacon_binary;

/// The default certification profile the proof records itself under.
///
/// Named rather than derived from the host, for the same reason `discovery-campaign` names
/// one: the profile is a claim about the environment a run happened in, and guessing it
/// would let two machines silently disagree about what a run is a claim about.
const DEFAULT_PROFILE: &str = "prof-linux-amd64-docker-0870";

/// The default seed.
///
/// Unlike a campaign's, this one **is** defaulted, and the difference is principled. A
/// campaign's seed is its reproducibility input: defaulting it would let every unattended
/// run share one stream and explore the same neighbourhood forever while reporting volume
/// as coverage. The proof does not explore — it needs *any* candidate deacon accepts, and
/// it plants the difference itself. A fixed default therefore makes the proof reproducible
/// by default, which is what a machinery assertion wants; `--seed` still overrides it, and
/// the value used is recorded in the report either way.
const DEFAULT_SEED: &str = "0x02542a1f";

/// How many candidates may be drawn while looking for one deacon accepts and that compares
/// clean against itself.
const DEFAULT_MAX_DRAWS: u64 = 64;

/// Probes minimization may spend per injection.
const DEFAULT_SHRINK_BUDGET: u64 = 24;

/// The per-invocation bound. `read-configuration` returns in about a second; a minute is
/// generous enough that a bound is never the reason a proof fails, and short enough that a
/// hung deacon does not hang a lane.
const INVOCATION_BOUND: Duration = Duration::from_secs(60);

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

    let report_root = workspace_root().join("target").join("discovery");
    let out = parsed
        .out
        .clone()
        .unwrap_or_else(|| report_root.join("proof.json"));

    let request = ProofRequest {
        deacon_binary: deacon,
        registry_dir: default_registry_dir(),
        report_root,
        seed_hex: parsed.seed_hex.clone(),
        seed: parsed.seed,
        profile: parsed.profile.clone(),
        bound: INVOCATION_BOUND,
        shrink_budget: parsed.shrink_budget,
        max_draws: parsed.max_draws,
    };

    // The FR-070 capability, taken out HERE and nowhere else in this program. Held as a
    // value rather than a bare call so the one place a process becomes able to inject is
    // visible in its `main`.
    let capability = RegressionHarness::declare();

    let report = match runtime.block_on(pipeline_proof::run(&request, &capability)) {
        Ok(report) => report,
        Err(e) => {
            // A prerequisite failure, a normalization failure, or a baseline that could not
            // be established. Distinct from a failed traversal and reported as such: the
            // pipeline was not exercised at all, which is not the same as it being broken.
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    let rendered = match report.render() {
        Ok(rendered) => rendered,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    if let Some(parent) = out.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("error: could not create {}: {e}", parent.display());
        return ExitCode::from(1);
    }
    if let Err(e) = std::fs::write(&out, &rendered) {
        eprintln!("error: could not write {}: {e}", out.display());
        return ExitCode::from(1);
    }

    // stdout: the single JSON document. stderr: everything else, so a caller can pipe the
    // report into `jq` without the diagnostics landing in it.
    print!("{rendered}");

    for traversal in &report.injections {
        match &traversal.verdict {
            TraversalVerdict::Traversed => eprintln!(
                "{} [{}] traversed all {} stage(s) as {}",
                traversal.injection,
                traversal.channel,
                traversal.stages.len(),
                traversal.signature.as_deref().unwrap_or("<no signature>")
            ),
            TraversalVerdict::FailedToSurface { stage, cause } => eprintln!(
                "PIPELINE DEFECT: {} [{}] landed on {} artifact(s) and stopped at `{}`: {cause}",
                traversal.injection,
                traversal.channel,
                traversal.applied,
                stage.as_str()
            ),
            TraversalVerdict::InjectionInapplicable { cause } => eprintln!(
                "PROOF DEFECT: {} [{}] never landed: {cause}. This says nothing about the \
                 pipeline — a perturbation that was not applied is a mis-authored record, \
                 not a campaign that found nothing.",
                traversal.injection, traversal.channel
            ),
        }
    }
    eprintln!(
        "proof: {} injection(s) over baseline candidate {} (seed {}); {} failed to surface, \
         {} inapplicable; report at {}",
        report.injections.len(),
        report.baseline_candidate,
        report.seed,
        report.failed_count,
        report.inapplicable_count,
        out.display()
    );

    ExitCode::from(report.exit_status())
}

/// The parsed invocation.
struct Parsed {
    seed_hex: String,
    seed: u64,
    profile: String,
    max_draws: u64,
    shrink_budget: u64,
    out: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut seed_hex = DEFAULT_SEED.to_string();
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut max_draws = DEFAULT_MAX_DRAWS;
    let mut shrink_budget = DEFAULT_SHRINK_BUDGET;
    let mut out: Option<PathBuf> = None;

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
            "--seed" => seed_hex = value()?,
            "--profile" => profile = value()?,
            "--candidates" => {
                let raw = value()?;
                max_draws = raw
                    .parse()
                    .map_err(|e| format!("--candidates {raw:?} is not a number: {e}"))?;
            }
            "--shrink-budget" => {
                let raw = value()?;
                shrink_budget = raw
                    .parse()
                    .map_err(|e| format!("--shrink-budget {raw:?} is not a number: {e}"))?;
            }
            "--out" => out = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    let seed = parse_seed(&seed_hex)?;
    if max_draws == 0 {
        return Err(
            "--candidates must be at least 1: a proof that may draw nothing can \
                    never establish the baseline it needs"
                .to_string(),
        );
    }
    if shrink_budget == 0 {
        return Err(
            "--shrink-budget must be at least 1: a reduction that may spend no \
                    probe never runs, and the minimization stage would report a defect for \
                    an invocation that forbade it"
                .to_string(),
        );
    }

    Ok(Parsed {
        seed_hex,
        seed,
        profile,
        max_draws,
        shrink_budget,
        out,
    })
}

/// Parse a seed, accepting `0x`-prefixed hex or plain hex.
///
/// Rejected rather than hashed into a `u64` on failure, for the same reason
/// `discovery-campaign` rejects one: a seed that silently became some other number would
/// record a value that does not reproduce the run it names.
fn parse_seed(raw: &str) -> Result<u64, String> {
    let trimmed = raw.trim();
    let digits = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    u64::from_str_radix(digits, 16)
        .map_err(|e| format!("--seed {raw:?} is not a 64-bit hex value: {e}"))
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    eprintln!(
        "usage: discovery-proof [--seed <hex>] [--profile <prof-id>] [--candidates <n>] \
         [--shrink-budget <n>] [--out <file>]"
    );
    ExitCode::from(2)
}
