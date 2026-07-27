//! Live discovery campaign binary (025-exploratory-parity-discovery, US1).
//!
//! **Selected ONLY by `[profile.discovery]`.** Every other profile — `default`,
//! `dev-fast`, `full`, `ci`, `mvp-integration`, `parity` — excludes it in its
//! `default-filter`, so those lanes are truthful by *non-selection*: a green pull-request
//! run never implies a campaign ran (FR-055/FR-057).
//!
//! ## What runs here, and what it needs
//!
//! Every test below drives a **real** campaign against the **verified pinned oracle**.
//! There is no mock and no skip: a missing or wrong-version oracle fails the test naming
//! the cause (FR-003), which is the whole point — a harness that quietly passed without
//! the reference would certify nothing while looking green.
//!
//! Each test owns an isolated temporary discovery root, so a campaign here never touches
//! the committed `conformance/discovery/` tree. That is not merely hygiene: the committed
//! queue is a reviewed artifact, and a test that appended to it would make `git status`
//! the review surface for machine output nobody looked at.
//!
//! ## Why this file exists at Phase 2 rather than at US1
//!
//! The lane wiring (T006/T007/T121) and this binary (T040) are mutually dependent:
//! nextest **hard-fails** a whole config when a `binary(=NAME)` predicate names a binary
//! that does not exist, so wiring the allow-list before the file exists would break every
//! lane in the workspace, not just this one. The file therefore landed with the wiring,
//! carrying the selection guard; US1 fills in the campaign acceptance tests.

use std::path::{Path, PathBuf};
use std::time::Duration;

use deacon_conformance::default_registry_dir;
use deacon_conformance::discovery::generate::Generator;
use deacon_conformance::discovery::grammar::Grammar;
use deacon_conformance::discovery::mutate;
use deacon_conformance::discovery::queue::{
    Budget, CampaignLane, CampaignTier, DEFAULT_ADMISSION_CAP,
    DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC,
};
use deacon_conformance::discovery::report::{
    TRIVIAL_FAILURE_CEILING, build_campaign_outcome_report,
};

use parity_harness::HarnessError;
use parity_harness::discovery::campaign::{self, CampaignRequest, CampaignRun};
use parity_harness::discovery::differential::{
    self, Characterization, DifferentialInput, OutcomeClass,
};
use parity_harness::oracle::{Oracle, VerifiedOracle};

/// The environment variable nextest sets to the profile it selected the run under.
const NEXTEST_PROFILE: &str = "NEXTEST_PROFILE";

/// The one profile permitted to select this binary.
const DISCOVERY_PROFILE: &str = "discovery";

/// The certification profile these campaigns record themselves under.
const TEST_PROFILE: &str = "prof-linux-amd64-docker-0870";

/// If this binary is running under a nextest profile at all, that profile must be
/// `discovery`.
///
/// This is the lane-isolation invariant enforced from **inside** the thing being isolated,
/// which is a stronger claim than the config cross-check in `discovery_hermetic`: that one
/// asserts the filters *say* the right thing, this one asserts the binary *was not
/// actually selected* by anything else. A future edit that widened a pull-request lane's
/// filter would fail here even if the config still parsed and the cross-check still passed
/// against some other reading of it.
///
/// An absent `NEXTEST_PROFILE` means the binary was run directly (`cargo test --test
/// discovery_campaign`), which is a deliberate developer act rather than a lane, and is
/// allowed. It is **not** a silent skip: the assertion below has nothing to assert about a
/// run that no profile selected.
#[test]
fn this_binary_runs_only_under_the_discovery_profile() {
    let Some(profile) = std::env::var_os(NEXTEST_PROFILE) else {
        // Run outside nextest: no profile selected it, so there is no selection to check.
        return;
    };
    let profile = profile.to_string_lossy().into_owned();
    assert_eq!(
        profile, DISCOVERY_PROFILE,
        "a live discovery campaign binary was selected by [profile.{profile}]. Only \
         [profile.{DISCOVERY_PROFILE}] may select it — every other lane must be truthful by \
         non-selection, so that a green pull-request run never implies a campaign ran \
         (FR-055/FR-057). Fix the profile's `default-filter` in .config/nextest.toml."
    );
}

// ---------------------------------------------------------------------------
// Shared scaffolding
// ---------------------------------------------------------------------------

/// An isolated discovery data root: the three empty collection files a campaign expects.
struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    fn new() -> Scratch {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["findings.json", "campaigns.json", "corpus.json"] {
            std::fs::write(
                dir.path().join(name),
                "{\n  \"schemaVersion\": 1,\n  \"records\": []\n}\n",
            )
            .expect("seed an empty collection");
        }
        Scratch { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// The deacon binary under test — the artifact cargo just built for this test binary,
/// never a path guessed from `target/`.
fn deacon_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_deacon"))
}

/// A campaign request over an isolated data root.
fn campaign_request(
    seed_hex: &str,
    seed: u64,
    candidates: u64,
    scratch: &Scratch,
) -> CampaignRequest {
    CampaignRequest {
        seed_hex: seed_hex.to_string(),
        seed,
        tier: CampaignTier::ConfigDifferential,
        lane: CampaignLane::Invoked,
        profile: TEST_PROFILE.to_string(),
        budget: Budget {
            // Generous: these tests bound themselves by candidate count, not by the clock,
            // so a slow machine produces a slower run rather than a different one.
            wall_clock_seconds: 1800,
            per_candidate_seconds: DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC,
            shrink_steps_per_finding: 64,
            admission_cap: DEFAULT_ADMISSION_CAP,
        },
        planned_candidates: candidates,
        registry_dir: default_registry_dir(),
        discovery_dir: scratch.path().to_path_buf(),
        report_root: scratch.path().join("artifacts"),
        deacon_binary: deacon_binary(),
        oracle_override: None,
        persist: true,
    }
}

/// Run a campaign, failing the test with the cause on any prerequisite problem.
///
/// `.expect` rather than a skip: a campaign that cannot verify the oracle has certified
/// nothing, and reporting that as a pass is the exact silent-vacuity failure FR-003
/// forbids.
fn run_campaign(request: &CampaignRequest) -> CampaignRun {
    runtime()
        .block_on(campaign::run(request))
        .unwrap_or_else(|e| {
            panic!(
                "the campaign could not run: {e}\n\nThis lane requires the verified pinned \
             oracle. Install it with `npm i -g @devcontainers/cli@<pinned version>` (see \
             fixtures/parity-corpus/oracle.json), or point DEACON_PARITY_DEVCONTAINER at \
             it. It is never skipped."
            )
        })
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("async runtime")
}

fn verified_oracle() -> VerifiedOracle {
    runtime()
        .block_on(Oracle::acquire())
        .expect("the verified pinned oracle is required by this lane and is never skipped")
}

// ---------------------------------------------------------------------------
// T025 — seed reproduction (SC-001)
// ---------------------------------------------------------------------------

/// The same seed and pinned input set produce an identical ordered candidate sequence and
/// an identical finding set.
///
/// This is the property every recorded finding's reproducibility rests on: a finding names
/// a campaign, a campaign names a seed, and if the seed does not reproduce the run then
/// the finding names nothing anyone can re-examine.
///
/// Asserted at both levels deliberately. The candidate sequence is checked hermetically
/// (no oracle needed) because a failure there localizes instantly to the generator; the
/// finding set is checked live, because the generator reproducing while the campaign does
/// not would mean the non-determinism is downstream — in normalization, in the tolerance
/// index, or in the admission order — and only the live comparison can see that.
#[test]
fn the_same_seed_reproduces_the_candidate_sequence_and_the_finding_set() {
    const SEED: u64 = 0x5EED_0025;

    // 1. The ordered candidate sequence, hermetically.
    let grammar = Grammar::load_default().expect("the committed grammar loads");
    let mut left = Generator::new(&grammar, SEED);
    let mut right = Generator::new(&grammar, SEED);
    let left_ids: Vec<String> = (0..60).map(|_| left.next_candidate().id).collect();
    let right_ids: Vec<String> = (0..60).map(|_| right.next_candidate().id).collect();
    assert_eq!(
        left_ids, right_ids,
        "the same seed must produce the identical ORDERED candidate sequence"
    );

    // 2. The finding set, live. Two isolated roots, so both runs start from an empty queue
    //    — comparing a second run that inherited the first's findings would prove nothing.
    let first_scratch = Scratch::new();
    let second_scratch = Scratch::new();
    let first = run_campaign(&campaign_request("0x5eed0025", SEED, 6, &first_scratch));
    let second = run_campaign(&campaign_request("0x5eed0025", SEED, 6, &second_scratch));

    assert_eq!(
        first.campaign.id, second.campaign.id,
        "the campaign id is derived from the seed and the pinned input set, so two runs of \
         the same seed against the same pins are the same campaign"
    );

    let mut first_findings: Vec<String> = first.findings.iter().map(|f| f.id.clone()).collect();
    let mut second_findings: Vec<String> = second.findings.iter().map(|f| f.id.clone()).collect();
    first_findings.sort();
    second_findings.sort();
    assert_eq!(
        first_findings, second_findings,
        "the same seed must produce the identical finding set"
    );

    assert_eq!(
        first.report.candidates_generated, second.report.candidates_generated,
        "reproducibility covers the volume too, or the two runs explored different spaces"
    );
    assert_eq!(
        first.report.mutation_applications, second.report.mutation_applications,
        "the per-category application counts are part of what a seed reproduces"
    );

    // A different seed must not alias, or "reproducible" would be trivially true because
    // every campaign produced the same thing.
    let other_scratch = Scratch::new();
    let other = run_campaign(&campaign_request("0x5eed0026", SEED + 1, 6, &other_scratch));
    assert_ne!(
        other.campaign.id, first.campaign.id,
        "a different seed is a different campaign"
    );
    let other_ids: Vec<String> = {
        let mut g = Generator::new(&grammar, SEED + 1);
        (0..60).map(|_| g.next_candidate().id).collect()
    };
    assert_ne!(other_ids, left_ids, "a different seed must not alias");
}

// ---------------------------------------------------------------------------
// T026 — the trivial-failure ceiling (SC-002)
// ---------------------------------------------------------------------------

/// At most 10% of generated candidates fail at the document-syntax stage.
///
/// The ceiling exists because a campaign whose candidates die before configuration
/// resolution is exploring the *parser*, not the tool — and it spends the expensive step
/// (an oracle invocation) to learn nothing. The ratio is reported for every run whether or
/// not it breaches, so a rising trend is visible before it matters.
#[test]
fn the_trivial_failure_ratio_stays_below_the_declared_ceiling() {
    let scratch = Scratch::new();
    // Forty candidates rather than a handful: the ceiling is a *proportion*, and over a
    // sample of five a single unlucky candidate is 20%. A ratio measured over a sample too
    // small to hold it is not a measurement.
    let run = run_campaign(&campaign_request("0x5eed0026", 0x5EED_0026, 40, &scratch));

    assert!(
        run.report.candidates_generated >= 40,
        "the ratio is meaningless over a sample the campaign never took: {} generated",
        run.report.candidates_generated
    );
    assert!(
        run.report.trivial_failure_fraction <= TRIVIAL_FAILURE_CEILING,
        "{} of {} candidates failed at the document-syntax stage ({:.1}%), above the {:.0}% \
         ceiling — the campaign is exploring the parser rather than the tool (SC-002)",
        run.report.parse_stage_failures,
        run.report.candidates_generated,
        run.report.trivial_failure_fraction * 100.0,
        TRIVIAL_FAILURE_CEILING * 100.0,
    );
    assert!(
        !run.report.trivial_failure_ceiling_breached,
        "the report must agree with the ratio it computed"
    );
    // FR-007 requires the proportion to be REPORTED for every run, not only when it
    // breaches. A ceiling nobody can see the distance to is a ceiling nobody manages.
    assert_eq!(
        run.report.trivial_failure_ceiling, TRIVIAL_FAILURE_CEILING,
        "the report states the ceiling it was judged against"
    );
}

// ---------------------------------------------------------------------------
// T027 — mutation-category coverage (SC-003)
// ---------------------------------------------------------------------------

/// Every declared mutation category is applied at least once, and **all eleven keys are
/// present** in `mutationApplications` including zeroes.
///
/// The two halves are different claims and both matter. "Applied at least once" says the
/// campaign explored the whole catalogue. "All eleven keys present" says a category that
/// was *not* applied would be visible as an explicit zero rather than as an absence —
/// FR-010's point, and the difference between a reported generation deficiency and a
/// silent one.
#[test]
fn every_mutation_category_is_applied_and_every_key_is_reported() {
    let scratch = Scratch::new();
    let run = run_campaign(&campaign_request("0x5eed0027", 0x5EED_0027, 44, &scratch));

    let counts = &run.report.mutation_applications;
    assert_eq!(
        counts.len(),
        mutate::CATEGORY_COUNT,
        "the report must carry every declared category, got {:?}",
        counts.keys().collect::<Vec<&String>>()
    );
    let keys: Vec<&str> = counts.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        mutate::category_names(),
        "the key list is the catalogue's, in its declaration order"
    );
    assert!(
        run.report.unapplied_categories.is_empty(),
        "categories never applied in a {}-candidate campaign: {:?}. A category with zero \
         successful applications is a hole in what the campaign explored (SC-003).",
        run.report.candidates_generated,
        run.report.unapplied_categories
    );
    for (category, applications) in counts {
        assert!(
            *applications > 0,
            "`{category}` was reported with zero applications while the deficiency list was \
             empty — the two views of the same fact disagree"
        );
    }

    // The zero case is representable and reported, not merely absent: rebuild the report
    // from a record whose counts are all zero and confirm every key survives.
    let mut zeroed = run.campaign.clone();
    zeroed.outcome.mutation_applications = mutate::empty_application_counts();
    let zero_report = build_campaign_outcome_report(&zeroed, &[]);
    assert_eq!(
        zero_report.mutation_applications.len(),
        mutate::CATEGORY_COUNT
    );
    assert_eq!(
        zero_report.unapplied_categories.len(),
        mutate::CATEGORY_COUNT,
        "every category with zero applications is named as an explicit deficiency"
    );
}

// ---------------------------------------------------------------------------
// T028 — the oracle fails loudly (FR-003)
// ---------------------------------------------------------------------------

/// A missing or wrong-version oracle fails naming the cause, reports no findings, and
/// never skips.
///
/// The failure mode this guards is the comfortable one: a campaign that could not verify
/// the reference, reported an empty finding set, and exited zero would look exactly like a
/// campaign that found the two implementations in perfect agreement.
///
/// Unix-only because it drives the verification path with shell stubs. The property is not
/// platform-specific; the stubs are.
#[cfg(unix)]
#[test]
fn an_unverifiable_oracle_fails_naming_the_cause_and_reports_no_findings() {
    use std::os::unix::fs::PermissionsExt;

    fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write stub");
        let mut perms = std::fs::metadata(&path).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        path
    }

    let stubs = tempfile::tempdir().expect("tempdir");

    // 1. Wrong version.
    let scratch = Scratch::new();
    let mut wrong = campaign_request("0x5eed0028", 0x5EED_0028, 4, &scratch);
    wrong.oracle_override = Some(stub(
        stubs.path(),
        "wrong-version",
        "#!/bin/sh\necho 0.0.1\n",
    ));
    let err = runtime()
        .block_on(campaign::run(&wrong))
        .expect_err("a wrong-version oracle must fail the campaign");
    let message = err.to_string();
    assert!(
        matches!(err, HarnessError::OracleUnverified { .. }),
        "expected OracleUnverified, got {err:?}"
    );
    assert!(
        message.contains("0.0.1"),
        "the diagnosis must name the version it found: {message}"
    );
    assert_findings_untouched(&scratch);

    // 2. Missing binary.
    let scratch = Scratch::new();
    let mut missing = campaign_request("0x5eed0028", 0x5EED_0028, 4, &scratch);
    missing.oracle_override = Some(stubs.path().join("definitely-not-here"));
    let err = runtime()
        .block_on(campaign::run(&missing))
        .expect_err("a missing oracle must fail the campaign");
    assert!(
        matches!(err, HarnessError::OracleUnverified { .. }),
        "expected OracleUnverified, got {err:?}"
    );
    assert!(
        err.to_string().contains("definitely-not-here"),
        "the diagnosis must name the path it could not use: {err}"
    );
    assert_findings_untouched(&scratch);

    // 3. A binary that is not the reference at all.
    let scratch = Scratch::new();
    let mut garbage = campaign_request("0x5eed0028", 0x5EED_0028, 4, &scratch);
    garbage.oracle_override = Some(stub(
        stubs.path(),
        "not-a-version",
        "#!/bin/sh\necho hello there\n",
    ));
    let err = runtime()
        .block_on(campaign::run(&garbage))
        .expect_err("an unidentifiable oracle must fail the campaign");
    assert!(
        matches!(err, HarnessError::OracleUnverified { .. }),
        "expected OracleUnverified, got {err:?}"
    );
    assert_findings_untouched(&scratch);
}

/// The queue must be untouched after a prerequisite failure: no findings, no campaign.
///
/// An empty finding set written by a campaign that never compared anything would be a
/// record of an observation nobody made.
#[cfg(unix)]
fn assert_findings_untouched(scratch: &Scratch) {
    let data = deacon_conformance::discovery::queue::DiscoveryData::load(scratch.path())
        .expect("the scratch root loads");
    assert!(
        data.findings.is_empty(),
        "a campaign that could not verify the reference reported findings: {:?}",
        data.findings
            .iter()
            .map(|f| &f.id)
            .collect::<Vec<&String>>()
    );
    assert!(
        data.campaigns.is_empty(),
        "a campaign that never ran must not record itself as having run"
    );
}

// ---------------------------------------------------------------------------
// T029 — budget exhaustion (FR-005)
// ---------------------------------------------------------------------------

/// An exhausted budget stops the campaign, sets `budgetExhausted`, and reports the portion
/// of the planned space it covered.
///
/// The alternative — presenting a truncated run as complete — is the failure FR-005 names:
/// a report that says "no findings" after covering 2% of its plan is making a much smaller
/// claim than it appears to.
#[test]
fn an_exhausted_budget_stops_the_campaign_and_reports_the_covered_fraction() {
    let scratch = Scratch::new();
    let mut request = campaign_request("0x5eed0029", 0x5EED_0029, 5_000, &scratch);
    // One second of wall clock against a five-thousand-candidate plan: the run cannot
    // finish, so exhaustion is the outcome under test rather than a race.
    request.budget.wall_clock_seconds = 1;

    let run = run_campaign(&request);

    assert!(
        run.report.budget_exhausted,
        "a campaign that stopped short of its plan must say so: {} of {} candidates",
        run.report.candidates_generated, request.planned_candidates
    );
    assert!(
        run.report.candidates_generated < request.planned_candidates,
        "the run reported exhaustion after covering its whole plan"
    );
    assert!(
        run.report.space_covered_fraction.is_finite(),
        "a non-finite fraction serializes as bare `null` and never loads back"
    );
    assert!(
        run.report.space_covered_fraction > 0.0 && run.report.space_covered_fraction < 1.0,
        "the covered fraction must be a real portion, got {}",
        run.report.space_covered_fraction
    );
    let expected = run.report.candidates_generated as f64 / request.planned_candidates as f64;
    assert!(
        (run.report.space_covered_fraction - expected).abs() < 1e-9,
        "the fraction must be the portion actually covered: reported {}, computed {expected}",
        run.report.space_covered_fraction
    );

    // Exhaustion is a fact about the run, never an error: the status reflects whether the
    // campaign ran, not what it found or how far it got (FR-058).
    assert!(
        run.report.candidates_generated > 0,
        "even an exhausted run reports the volume it did reach"
    );

    // And a run that completes its plan is NOT exhausted, or the flag would say nothing.
    let complete_scratch = Scratch::new();
    let complete = run_campaign(&campaign_request(
        "0x5eed002a",
        0x5EED_002A,
        3,
        &complete_scratch,
    ));
    assert!(!complete.report.budget_exhausted);
    assert!((complete.report.space_covered_fraction - 1.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// T067 — the admission cap (FR-034b, SC-019)
// ---------------------------------------------------------------------------

/// A campaign that exceeds its admission cap admits at most the cap, reports a **non-zero**
/// suppressed count, and still succeeds.
///
/// All three clauses carry weight and none implies the others:
///
/// - **At most the cap** is what keeps the queue reviewable. It is set from reviewer
///   throughput, not machine capacity (research D10): a nightly run that admits more new
///   signatures than anyone clears before the next run has produced a backlog, not
///   coverage.
/// - **A non-zero suppressed count** is what keeps the cap honest. Silent truncation would
///   render "we found 25 things" and "we found many more than we can review" identically —
///   and the second is itself a signal that something systemic is diverging, which is
///   precisely the signal a discovery lane exists to raise.
/// - **Still succeeds** is what keeps discovery from gating on its own output. A campaign
///   that failed when it found too much would make green depend on finding little, and the
///   quickest route to green would be to break the generator.
///
/// The cap is set to `1` rather than left at the default so the boundary is *reached*: at
/// 25 this test would depend on the two implementations disagreeing two dozen ways, and a
/// run that admitted 3 findings would pass while asserting nothing at all.
#[test]
fn exceeding_the_admission_cap_suppresses_visibly_and_still_succeeds() {
    let scratch = Scratch::new();
    let mut request = campaign_request("0x5eed0067", 0x5EED_0067, 60, &scratch);
    request.budget.admission_cap = 1;

    // `run_campaign` panics on any prerequisite failure, so reaching the next line IS the
    // "still succeeds" clause: a campaign that found more than it could admit ran to
    // completion rather than erroring.
    let run = run_campaign(&request);

    assert!(
        run.report.signatures_admitted <= request.budget.admission_cap,
        "the campaign admitted {} signature(s) against a cap of {}",
        run.report.signatures_admitted,
        request.budget.admission_cap
    );
    assert!(
        run.report.signatures_suppressed > 0,
        "the campaign observed {} distinct signature(s) and admitted {}, yet reported zero \
         suppressed. Either the cap did not engage — in which case this test asserts \
         nothing — or suppression is silent, which is the failure FR-034b exists to \
         prevent. Volume: {} generated, {} executed.",
        run.report.signatures_observed,
        run.report.signatures_admitted,
        run.report.candidates_generated,
        run.report.candidates_executed
    );
    assert!(
        run.report.signatures_admitted + run.report.signatures_suppressed
            <= run.report.signatures_observed,
        "admitted ({}) + suppressed ({}) cannot exceed observed ({}) — every suppressed \
         signature was observed first",
        run.report.signatures_admitted,
        run.report.signatures_suppressed,
        run.report.signatures_observed
    );

    // The persisted queue holds no more than the cap, so the truncation is real and not
    // only reported.
    let data = deacon_conformance::discovery::queue::DiscoveryData::load(scratch.path())
        .expect("the written data root must load");
    assert!(
        data.findings.len() as u64 <= request.budget.admission_cap,
        "the queue holds {} finding(s) against a cap of {}",
        data.findings.len(),
        request.budget.admission_cap
    );

    // And the suppression reaches the artifacts a reviewer actually reads — both the
    // campaign's own report and the standing queue report, which totals suppression across
    // every campaign so the queue is legible as a *sample*.
    let json = deacon_conformance::discovery::report::render_campaign_json(&run.report);
    let document: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(
        document["signaturesSuppressed"],
        serde_json::json!(run.report.signatures_suppressed),
        "the emitted document must carry the suppressed count, not only the in-memory report"
    );

    let registry =
        deacon_conformance::load::Registry::load(&default_registry_dir()).expect("registry loads");
    let pins = deacon_conformance::discovery::report::CurrentPins::from_registry(&registry);
    let queue_report = deacon_conformance::discovery::report::build_queue_report(&data, &pins);
    assert_eq!(
        queue_report.signatures_suppressed, run.report.signatures_suppressed,
        "the standing queue report must total the campaign's suppression"
    );
    let markdown = deacon_conformance::discovery::report::render_md(&queue_report);
    assert!(
        markdown.contains("This queue is a sample"),
        "the human report must say that the counts above it are not everything: {markdown}"
    );

    // The cap is a *cap*, not a fixed size: the same seed with a cap it cannot reach admits
    // strictly more, so the numbers above are the cap engaging rather than the run simply
    // finding one thing.
    //
    // The comparison cap is set far above the default, not left at it: the default of 25 is
    // reviewer throughput, and this seed genuinely exceeds it (the first attempt at this
    // test suppressed 4 signatures in the comparison run), which would have made the
    // comparison itself truncated and the conclusion unsound.
    let uncapped_scratch = Scratch::new();
    let mut uncapped_request = campaign_request("0x5eed0067", 0x5EED_0067, 60, &uncapped_scratch);
    uncapped_request.budget.admission_cap = 10_000;
    assert!(
        uncapped_request.budget.admission_cap > DEFAULT_ADMISSION_CAP,
        "the comparison run must not be capped at reviewer throughput"
    );
    let uncapped = run_campaign(&uncapped_request);
    assert!(
        uncapped.report.signatures_admitted > run.report.signatures_admitted,
        "the same seed uncapped admitted {} and capped admitted {} — if they are equal the \
         cap never engaged",
        uncapped.report.signatures_admitted,
        run.report.signatures_admitted
    );
    assert_eq!(
        uncapped.report.signatures_suppressed, 0,
        "the comparison run must not itself be truncated, or it says nothing about what the \
         capped run left out"
    );
}

// ---------------------------------------------------------------------------
// T122 — outcomes and structured content, never message wording (FR-016)
// ---------------------------------------------------------------------------

/// Two rejections that differ only in wording produce **no finding**.
///
/// Both implementations refuse a malformed document, and both explain themselves in their
/// own words — different words, in different formats, on different streams. If diagnostic
/// prose were part of the comparison, every one of those would be a finding, and the queue
/// would be a list of the two projects' writing styles.
///
/// The property is structural rather than filtered: there is no code path from a captured
/// stderr into a comparison. This test observes the consequence, and additionally asserts
/// that the two messages really did differ — otherwise it would pass vacuously on a day
/// when both sides happened to say the same thing.
#[test]
fn two_rejections_that_differ_only_in_wording_are_not_a_finding() {
    let oracle = verified_oracle();
    let scratch = Scratch::new();
    // A workspace with no configuration at all. Both implementations refuse it, and each
    // explains itself in its own words on its own stream.
    //
    // Note what this fixture is deliberately NOT: a malformed `devcontainer.json`. That
    // looks like the obvious choice and is the wrong one — the reference's
    // `read-configuration` is a lenient parse-and-echo that ACCEPTS malformed JSONC where
    // deacon rejects it, a divergence this project has already characterized
    // (`bhv-readconfig-malformed-jsonc-rejected`). Using it here would produce a genuine
    // outcome difference and this test would fail for the right reason at the wrong
    // question. An absent configuration is the case both sides really do refuse.
    let workspace = tempfile::tempdir().expect("tempdir");

    let result = runtime()
        .block_on(differential::compare(
            DifferentialInput {
                candidate_id: "cnd-wording",
                workspace: workspace.path(),
                deacon: &deacon_binary(),
                oracle: &oracle,
                bound: Duration::from_secs(60),
                report_root: &scratch.path().join("artifacts"),
                deliberately_invalid: true,
            },
            &Characterization::default(),
        ))
        .expect("the comparison itself must succeed");

    assert_eq!(result.deacon.outcome, OutcomeClass::Rejected);
    assert_eq!(result.reference.outcome, OutcomeClass::Rejected);
    assert!(
        result.parse_stage_failure,
        "neither side reached configuration resolution, which is what a document-syntax \
         failure is"
    );
    assert!(
        result.observations.is_empty(),
        "two rejections produced {} observation(s): {:?}. Only outcomes and structured \
         content are compared — never diagnostic message wording (FR-016).",
        result.observations.len(),
        result
            .observations
            .iter()
            .map(|o| (&o.signature.channel, &o.signature.path))
            .collect::<Vec<_>>()
    );

    // The messages really did differ, so the assertion above is not vacuous.
    let deacon_stderr =
        std::fs::read_to_string(&result.deacon.stderr_path).expect("deacon stderr preserved");
    let reference_stderr =
        std::fs::read_to_string(&result.reference.stderr_path).expect("reference stderr preserved");
    assert!(
        !deacon_stderr.trim().is_empty(),
        "deacon explained its rejection somewhere; if not, this test proves nothing"
    );
    assert!(
        !reference_stderr.trim().is_empty(),
        "the reference explained its rejection somewhere; if not, this test proves nothing"
    );
    assert_ne!(
        deacon_stderr.trim(),
        reference_stderr.trim(),
        "the two rejections must actually differ in wording, or the comparison had nothing \
         to ignore"
    );

    // Raw evidence is preserved SEPARATELY from the normalized form (FR-014). Here there
    // is no normalized document at all, and the raw bytes are still on disk — which is
    // what lets a reviewer see the wording the comparison declined to read.
    assert!(result.deacon.normalized.is_none());
    assert!(result.reference.normalized.is_none());
    assert!(result.deacon.stdout_path.is_file());
    assert!(result.reference.stdout_path.is_file());
}

// ---------------------------------------------------------------------------
// T123 — zero-finding volume reporting (FR-062)
// ---------------------------------------------------------------------------

/// A campaign that finds nothing still reports `candidatesGenerated` and
/// `candidatesExecuted`, so "nothing found" is distinguishable from "nothing ran".
///
/// Without the volume, a broken pipeline is the most comfortable possible state for this
/// machinery to be in: it reports exactly what a clean run reports.
///
/// Constructed from a **real** campaign rather than a synthetic record. A campaign against
/// the current pins does find differences, so the zero-finding case is produced by
/// rebuilding that run's report with an empty finding list — the volume comes from a run
/// that genuinely happened, and the question under test is whether the report keeps it when
/// there is nothing to show. A purely synthetic record would test the constructor rather
/// than the campaign.
#[test]
fn a_campaign_that_finds_nothing_still_reports_what_it_covered() {
    let scratch = Scratch::new();
    let run = run_campaign(&campaign_request("0x5eed0123", 0x5EED_0123, 8, &scratch));

    // The run really ran.
    assert!(run.report.candidates_generated > 0);
    assert!(run.report.candidates_executed > 0);

    let nothing_found = build_campaign_outcome_report(&run.campaign, &[]);
    assert!(nothing_found.findings.is_empty());
    assert_eq!(
        nothing_found.candidates_generated, run.report.candidates_generated,
        "the volume must survive having nothing to report"
    );
    assert_eq!(
        nothing_found.candidates_executed,
        run.report.candidates_executed
    );

    let json = deacon_conformance::discovery::report::render_campaign_json(&nothing_found);
    let document: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(
        document["candidatesGenerated"], run.report.candidates_generated,
        "the emitted document carries the volume, not only the in-memory report"
    );
    assert_eq!(
        document["candidatesExecuted"],
        run.report.candidates_executed
    );
    assert_eq!(document["findings"], serde_json::json!([]));

    // And the two facts are genuinely distinguishable: a campaign that never ran reports a
    // different volume, so a reader can tell them apart.
    let mut never_ran = run.campaign.clone();
    never_ran.outcome.candidates_generated = 0;
    never_ran.outcome.candidates_executed = 0;
    let never_ran = build_campaign_outcome_report(&never_ran, &[]);
    assert_ne!(
        never_ran.candidates_generated, nothing_found.candidates_generated,
        "\"nothing found\" and \"nothing ran\" must not render identically — that \
         indistinguishability is the whole of FR-062"
    );

    let markdown = deacon_conformance::discovery::report::render_campaign_md(&nothing_found);
    assert!(
        markdown.contains("admitted no findings"),
        "the human report must say what the absence does not mean"
    );
    assert!(markdown.contains(&run.report.candidates_generated.to_string()));
}
