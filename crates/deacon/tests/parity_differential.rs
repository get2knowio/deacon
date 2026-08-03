//! The DIFFERENTIAL parity lane: every `live-differential` case, run against the
//! verified pinned reference CLI.
//!
//! This is the lane that answers "does deacon behave like the CLI?" by running BOTH and
//! diffing. It needs the pinned oracle installed, so it runs ONLY under
//! `cargo nextest run --profile parity` (nightly). Its two siblings need no oracle and
//! gate pull requests instead: `parity_hermetic` (declared expectations, no Docker) and
//! `parity_docker` (declared expectations, Docker-backed).
//!
//! There is no opt-in env gate and no silent skip: a missing or mismatched oracle, absent
//! Docker, a missing fixture, or a normalization failure FAILS with a cause-specific
//! message (constitution IV).
//!
//! **One driver function per `resourceGroup`.** All four groups appear here, because
//! whether a case needs the reference is independent of whether it needs a daemon — 67 of
//! its cases are config-only and 49 are Docker-backed. Adding a case with an existing
//! group is a pure data edit (SC-013).
//!
//! **Two bounds, doing different jobs.** Each case is bounded at five minutes inside the
//! shared runner (FR-077b), so a wedged case is named and the rest of the group still runs.
//! The Docker tier as a whole is bounded at thirty minutes (FR-077a) and asserted HERE,
//! explicitly, from its own measurements — not delegated to nextest's `slow-timeout`, which
//! would report only "the binary was slow" and name nothing to fix.

use std::path::PathBuf;
use std::sync::Arc;

use parity_harness::load::Registry;
use parity_harness::model::ResourceGroup;
use parity_harness::parity_root;

use parity_harness::driver::{self, DriverConfig, Lane, TIER_BUDGET};
use parity_harness::oracle::Oracle;
use parity_harness::prereq;
use parity_harness::{HarnessError, report_root, workspace_root};

/// This binary's name — the fragment key, the registry entry, and the tier-timing key.
const BINARY: &str = "parity_differential";

/// Fail the test with the error's cause-specific `Display` message (never `Debug`) so an
/// oracle/prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Drive one resource group's differential cases, then assert the Docker tier budget.
///
/// A group with no cases is NOT a silent skip: it still writes its fragment (which is what
/// proves the binary ran) and says so on stderr. The prerequisites are still required and
/// still checked — "no case today" must not quietly become "no oracle needed".
async fn drive(group: ResourceGroup) {
    // Prerequisites first, both fail-loud: an absent daemon or a mismatched oracle must
    // fail this lane, never skip it to a green. Docker is required only where the group
    // needs it; the oracle is required unconditionally, because that is what this lane IS.
    if driver::needs_docker(group) {
        ff(prereq::require_docker().await);
    }
    let oracle = ff(Oracle::acquire().await);
    let root = workspace_root();

    let registry = Registry::load(&parity_root())
        .unwrap_or_else(|e| panic!("the parity case set must load: {e}"));
    let cases = driver::cases_in_lane_and_group(&registry.cases, Lane::Differential, group);
    if cases.is_empty() {
        eprintln!(
            "note: no differential case declares resourceGroup `{}` yet",
            driver::group_slug(group)
        );
    }

    let cfg = Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        oracle: Some(oracle),
        fixtures_root: root.join("parity").join("fixtures"),
        report_root: report_root(),
    });

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, group).await);
    ff(driver::emit(&run));

    if driver::needs_docker(group) {
        // Record THIS group's wall clock before asserting, so the assertion below folds in
        // a complete picture of whatever has finished so far.
        ff(driver::record_timing(&cfg, &run).await);
        assert_tier_within_budget(&cfg);
    }

    assert!(
        run.failures.is_empty(),
        "differential divergence(s) in resource group `{}`:\n{}",
        driver::group_slug(group),
        run.failures.join("\n"),
    );
}

/// FR-077a / research Decision 10: the Docker tier's own measured wall clock must stay
/// within [`TIER_BUDGET`], and exceeding it fails with the NUMBER and the SLOWEST CASES.
///
/// The tier spans this binary's driver functions, which nextest runs as sibling processes.
/// Each records its group's span into a per-run artifact tree, and the fold below takes
/// `max(finished) - min(started)` across every artifact present — the true tier wall clock,
/// not the sum (which would over-count concurrent groups). A process that runs before its
/// siblings sees a partial fold, which can only UNDER-report; the last to finish sees the
/// whole tier. So the tier can never pass by measuring less than happened, and the run fails
/// in at least one test when the budget is genuinely blown.
fn assert_tier_within_budget(cfg: &DriverConfig) {
    let summary = ff(driver::tier_summary(&cfg.report_root, &cfg.binary));
    eprintln!(
        "docker conformance tier: {:?} elapsed across group(s) [{}], budget {:?}",
        summary.elapsed,
        summary.groups.join(", "),
        TIER_BUDGET
    );
    if let Some(violation) = driver::budget_violation(&summary, TIER_BUDGET) {
        panic!("{violation}");
    }
}

/// Resource group `none` — the config-only differential cases (`read-configuration`,
/// `doctor`, `build --output` and friends). The bulk of the lane.
#[tokio::test]
async fn differential_group_none() {
    drive(ResourceGroup::None).await;
}

/// Resource group `fs-heavy` — significant filesystem work, no Docker.
#[tokio::test]
async fn differential_group_fs_heavy() {
    drive(ResourceGroup::FsHeavy).await;
}

/// Resource group `docker-shared` — safe concurrent daemon use.
///
/// Every Docker case runs in its own isolated external temp workspace, so its
/// `devcontainer.local_folder` label (and therefore its container/network/volume names) is
/// unique; four cases run at a time without colliding.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn differential_group_docker_shared() {
    drive(ResourceGroup::DockerShared).await;
}

/// Resource group `docker-exclusive` — exclusive daemon access, driven serially.
///
/// A case lands here when it manipulates state the daemon shares across containers, so its
/// driver runs one case at a time even though the tier as a whole is concurrent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn differential_group_docker_exclusive() {
    drive(ResourceGroup::DockerExclusive).await;
}

// ---------------------------------------------------------------------------------
// The container-backed error-path tier (024 US4, T096/T098/T099)
// ---------------------------------------------------------------------------------

/// The registry's error-path cases, id-sorted.
fn error_path_cases() -> Vec<parity_harness::model::TestCase> {
    let registry = Registry::load(&parity_root())
        .unwrap_or_else(|e| panic!("the parity case set must load: {e}"));
    let mut cases: Vec<_> = registry
        .cases
        .iter()
        .filter(|c| c.error_path_tier)
        .cloned()
        .collect();
    cases.sort_by(|a, b| a.id.cmp(&b.id));
    assert!(
        !cases.is_empty(),
        "the error-path tier is empty; SC-007 requires a case for each later-stage failure \
         point"
    );
    cases
}

/// A `DriverConfig` for the tier tests below, with prerequisites already checked.
async fn tier_config(report_root: &std::path::Path) -> Arc<DriverConfig> {
    ff(prereq::require_docker().await);
    let oracle = ff(Oracle::acquire().await);
    let root = workspace_root();
    Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        oracle: Some(oracle),
        fixtures_root: root.join("parity").join("fixtures"),
        report_root: report_root.to_path_buf(),
    })
}

/// T096 (US4 scenario 1, FR-042): an error-path case records the failing STAGE and each
/// side's outcome, and reaches a definite verdict.
///
/// This is the tier's whole claim, and it is the one thing a green run does not by itself
/// establish. A case that never got past configuration read agrees too; so does one whose
/// declared later stage was never reached. What is asserted is therefore not "it passed"
/// but "the evidence says which stage, and what each side did there":
///
/// - the verdict is `agree` or `allowed-difference` — a definite verdict, never absent;
/// - the exit-code detail names the DECLARED stage, not a coarser inference of it;
/// - the operation that declared the failure really did fail on deacon's side (a declared
///   stage nothing reached would otherwise be a claim the run silently contradicts);
/// - for a differential, the REFERENCE's outcome is recorded too, so a divergence is a
///   statement about two observed behaviours rather than about one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_error_path_case_records_the_failing_stage_and_each_sides_outcome() {
    use parity_harness::evidence::Outcome;
    use parity_harness::model::{CHAN_EXIT_CODE, OracleType};

    let reports = tempfile::tempdir().expect("tempdir");
    let cfg = tier_config(reports.path()).await;
    let cases = error_path_cases();

    for case in &cases {
        let verdict = ff(parity_harness::runner::run_case(case, &cfg.run_config()).await);
        assert!(
            matches!(verdict.overall, Outcome::Agree | Outcome::AllowedDifference),
            "{}: reached `{}` — the tier's cases are characterized, so an uncharacterized \
             outcome means the later stage stopped behaving as recorded",
            case.id,
            verdict.overall.as_str()
        );

        for (op_id, declared) in case.later_stage_failure_phases() {
            // `ChannelVerdict` carries no operation id, and a case may declare the same
            // channel on several operations (the teardown case observes `chan-exit-code`
            // three times). The verdicts are produced in `case.expected` order, so the
            // DECLARING operation's verdict is found by resolving each expectation's target
            // — matching on channel alone would silently read a different operation's.
            let position = case
                .expected
                .iter()
                .position(|exp| {
                    exp.channel == CHAN_EXIT_CODE
                        && match &exp.operation {
                            Some(id) => id == op_id,
                            None => case.operations.last().map(|o| o.id.as_str()) == Some(op_id),
                        }
                })
                .unwrap_or_else(|| {
                    panic!(
                        "{}: operation {op_id:?} declares `{}` but no exit-code expectation \
                         observes it",
                        case.id,
                        declared.as_str()
                    )
                });
            let exit = verdict.channels.get(position).unwrap_or_else(|| {
                panic!(
                    "{}: expected {} channel verdicts, got {}",
                    case.id,
                    case.expected.len(),
                    verdict.channels.len()
                )
            });
            let detail = exit.detail.as_ref().unwrap_or_else(|| {
                panic!(
                    "{}: the exit-code verdict for operation {op_id:?} carries no detail",
                    case.id
                )
            });

            assert_eq!(
                detail.get("failurePhase").and_then(|v| v.as_str()),
                Some(declared.as_str()),
                "{}: operation {op_id:?} declares `{}` but the verdict records {:?}; the \
                 recorded stage must be the reviewed one (FR-042)",
                case.id,
                declared.as_str(),
                detail.get("failurePhase")
            );

            let sides = detail
                .get("sides")
                .unwrap_or_else(|| panic!("{}: verdict records no per-side outcome", case.id));
            assert_eq!(
                sides.pointer("/deacon/failed").and_then(|v| v.as_bool()),
                Some(true),
                "{}: declares a failure at `{}` that deacon's run did not produce — the \
                 stage is recorded and nothing reached it: {sides}",
                case.id,
                declared.as_str()
            );
            if case.oracle_type == Some(OracleType::LiveDifferential) {
                assert!(
                    sides
                        .get("reference")
                        .is_some_and(|r| r.get("failed").is_some()),
                    "{}: is a differential but records no outcome for the reference side, so \
                     its verdict states a disagreement without saying what the other side \
                     did (FR-042): {sides}",
                    case.id
                );
            }
        }
    }
}

/// T098 (US4 scenario 3, FR-045): every container, network, volume and temp directory is
/// reclaimed — on success AND on the failing runs the tier is made of.
///
/// An error-path case fails partway BY CONSTRUCTION, which is exactly the run where cleanup
/// is skipped: the teardown step never happens because the step before it did not finish.
/// `docker_channels.rs` proves the RAII guard fires on an unwind in isolation; what is
/// checked here is the property in situ, over the real tier.
///
/// **Measured by attribution, not by a global count.** The obvious check — total containers
/// before vs after — is wrong here for a reason worth recording: nextest's `parity` group
/// runs two test functions at a time, so a sibling driver is creating and destroying
/// resources throughout this window, and a global count reports its churn as this tier's
/// leak (and could hide a real leak behind its cleanup). Every leak this tier can produce is
/// instead identifiable by NAME: a container labelled with an isolated workspace path that no
/// longer exists on disk is orphaned by definition, and the harness's own resource names are
/// `dcr-<pid>-<seq>-…`. Both are checked as SET differences across the window, so a
/// pre-existing orphan is not blamed on this run.
///
/// **The temp directory is deliberately not measured here**, for the same reason. A
/// `deacon-conf-*` directory that appears during this window and is still present at the end
/// is indistinguishable from a sibling driver's IN-FLIGHT workspace — the first draft of this
/// test asserted on it and failed naming four directories that a concurrent group was still
/// using. Attribution needs the creating process, which a directory name does not carry. It
/// is covered instead where attribution is exact: `workspace.rs::tempdir_is_removed_on_drop`
/// (the guard removes it on drop) and `docker_channels.rs::cleanup_runs_on_unwind_drop` (the
/// drop happens on an unwind, which is the failing-run case this tier is made of). What
/// remains observable here — a *container* whose workspace directory is already gone — is
/// asserted above, and is the stronger half: it is the residue that survives the process.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_error_path_tier_reclaims_every_resource_it_creates() {
    let reports = tempfile::tempdir().expect("tempdir");
    let cfg = tier_config(reports.path()).await;
    let cases = error_path_cases();

    let before = residual_snapshot();
    for case in &cases {
        // The verdict is T096's subject; here only the side effects matter, so a failure to
        // run is still surfaced (fail-loud) but the outcome itself is not re-asserted.
        let _ = ff(parity_harness::runner::run_case(case, &cfg.run_config()).await);
    }
    let after = residual_snapshot();

    let leaked_containers = after
        .orphaned_containers
        .difference(&before.orphaned_containers);
    let leaked: Vec<&String> = leaked_containers.collect();
    assert!(
        leaked.is_empty(),
        "the tier left container(s) behind, each labelled with an isolated workspace that no \
         longer exists: {leaked:?}. A case that fails partway must still reclaim what it \
         created (FR-045)"
    );

    let leaked_named: Vec<&String> = after
        .harness_named
        .difference(&before.harness_named)
        .collect();
    assert!(
        leaked_named.is_empty(),
        "the tier left harness-named network(s)/volume(s) behind: {leaked_named:?}"
    );

    let leaked_compose: Vec<&String> = after
        .orphaned_compose
        .difference(&before.orphaned_compose)
        .collect();
    assert!(
        leaked_compose.is_empty(),
        "the tier left Compose network(s)/volume(s) behind, each named for an isolated \
         workspace that no longer exists: {leaked_compose:?}. These outlive both `deacon \
         down` (which computes a different project name) and the container label sweep \
         (only containers carry `devcontainer.local_folder`), and they accumulate until the \
         daemon answers a compose `up` with \"all predefined address pools have been fully \
         subnetted\" — a leak that surfaces as a flake in an unrelated case"
    );
}

/// T099 (US4 scenario 4, FR-045): two concurrent cases observe none of each other's
/// resources.
///
/// Driven by running the SAME case twice at once, which is the strongest form of the
/// question: identical fixture, identical configuration, therefore identical container
/// identity — UNLESS each run's isolated workspace really does make it different. The case
/// chosen creates a container and then tears it down, so a collision is not merely
/// theoretical: two runs sharing a `devcontainer.local_folder` label would have one run's
/// `down` remove the other's container, and the other's teardown assertions would then be
/// reporting on a container it never created.
///
/// Both runs must reach the same definite verdict they reach alone. A weaker check — that
/// the workspaces differ — would pass on a harness that computed distinct paths and then
/// labelled both containers identically anyway.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_error_path_cases_observe_none_of_each_others_resources() {
    use parity_harness::evidence::Outcome;

    let reports = tempfile::tempdir().expect("tempdir");
    let cfg = tier_config(reports.path()).await;
    let case = error_path_cases()
        .into_iter()
        .find(|c| {
            c.operations
                .iter()
                .any(|op| op.subcommand == "up" && op.expect_failure_phase.is_none())
        })
        .expect(
            "the tier needs a case whose `up` SUCCEEDS, or a collision could not manifest as \
             a shared container in the first place",
        );

    let a = {
        let (cfg, case) = (Arc::clone(&cfg), case.clone());
        tokio::spawn(
            async move { parity_harness::runner::run_case(&case, &cfg.run_config()).await },
        )
    };
    let b = {
        let (cfg, case) = (Arc::clone(&cfg), case.clone());
        tokio::spawn(
            async move { parity_harness::runner::run_case(&case, &cfg.run_config()).await },
        )
    };
    let a = ff(a.await.expect("concurrent case A completed"));
    let b = ff(b.await.expect("concurrent case B completed"));

    for (label, verdict) in [("A", &a), ("B", &b)] {
        assert!(
            matches!(verdict.overall, Outcome::Agree | Outcome::AllowedDifference),
            "concurrent run {label} of `{}` reached `{}`; run alone it does not, so the two \
             runs saw each other's resources",
            case.id,
            verdict.overall.as_str()
        );
    }
    assert_eq!(
        a.overall, b.overall,
        "two concurrent runs of `{}` disagreed with each other, which they can only do by \
         sharing something",
        case.id
    );
}

/// What the daemon and the temp tree hold that is ATTRIBUTABLE to the harness, for the
/// before/after set comparison in T098.
struct Residual {
    /// Containers whose `devcontainer.local_folder` label names an isolated workspace that
    /// no longer exists — orphaned by definition, whichever case created them.
    orphaned_containers: std::collections::BTreeSet<String>,
    /// Networks and volumes named by `DockerWorkspace::resource_name` (`dcr-<pid>-<seq>-…`).
    harness_named: std::collections::BTreeSet<String>,
    /// Networks and volumes a COMPOSE project rooted in an isolated workspace created,
    /// where that workspace no longer exists — orphaned by the same rule the containers
    /// use. Compose names every resource `<project>_<resource>` and the reference derives
    /// its project from the workspace DIRECTORY, so the workspace basename is the leading
    /// segment. These are invisible to `harness_named` (the harness did not name them) and
    /// to the container sweep (only containers carry `devcontainer.local_folder`), which is
    /// how 28 of them accumulated before anything noticed.
    orphaned_compose: std::collections::BTreeSet<String>,
}

/// The prefix `workspace.rs` gives every isolated workspace directory.
const WORKSPACE_PREFIX: &str = "deacon-conf-";
/// The prefix `DockerWorkspace::resource_name` gives every network/volume it names.
const HARNESS_RESOURCE_PREFIX: &str = "dcr-";

fn residual_snapshot() -> Residual {
    let mut orphaned_containers = std::collections::BTreeSet::new();
    for line in docker_lines(&[
        "ps",
        "-a",
        "--no-trunc",
        "--format",
        "{{.ID}}\t{{.Label \"devcontainer.local_folder\"}}",
    ]) {
        let Some((id, folder)) = line.split_once('\t') else {
            continue;
        };
        let folder = folder.trim();
        let is_isolated = std::path::Path::new(folder)
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(WORKSPACE_PREFIX));
        if is_isolated && !std::path::Path::new(folder).exists() {
            orphaned_containers.insert(format!("{} ({folder})", &id[..12.min(id.len())]));
        }
    }

    let mut harness_named = std::collections::BTreeSet::new();
    let mut orphaned_compose = std::collections::BTreeSet::new();
    let live_workspaces = live_isolated_workspaces();
    for (kind, args) in [
        ("network", vec!["network", "ls", "--format", "{{.Name}}"]),
        ("volume", vec!["volume", "ls", "--format", "{{.Name}}"]),
    ] {
        for name in docker_lines(&args) {
            if name.starts_with(HARNESS_RESOURCE_PREFIX) {
                harness_named.insert(format!("{kind}:{name}"));
            }
            if let Some(workspace) = compose_workspace_of(&name) {
                // An isolated workspace that still exists is a SIBLING driver's in-flight
                // run, not a leak — the same attribution rule the container sweep uses.
                if !live_workspaces.contains(&workspace) {
                    orphaned_compose.insert(format!("{kind}:{name}"));
                }
            }
        }
    }

    Residual {
        orphaned_containers,
        harness_named,
        orphaned_compose,
    }
}

/// The isolated-workspace directory name a Compose resource name carries, if any,
/// LOWERCASED for comparison against [`live_isolated_workspaces`].
///
/// Compose names every resource `<project>_<resource>`, and a project rooted in an isolated
/// workspace takes its name from that workspace's directory, so the leading `deacon-conf-…`
/// segment names the workspace. Returns `None` for anything else.
///
/// The case fold matters: a Compose project name is lowercased and `tempfile`'s suffix is
/// not, so comparing the raw segment to a directory name reports every in-flight sibling as
/// an orphan.
fn compose_workspace_of(name: &str) -> Option<String> {
    let (head, _) = name.split_once('_')?;
    head.starts_with(WORKSPACE_PREFIX)
        .then(|| head.to_ascii_lowercase())
}

/// The lowercased basenames of the isolated workspace directories that currently exist —
/// each one a run that is still in flight, whose Compose resources are not leaks.
fn live_isolated_workspaces() -> std::collections::BTreeSet<String> {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return std::collections::BTreeSet::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(WORKSPACE_PREFIX)
                .then(|| name.to_ascii_lowercase())
        })
        .collect()
}

/// The non-empty lines `docker <args>` printed. A probe that cannot run yields nothing on
/// BOTH sides of the comparison, so it can never manufacture a pass by shrinking the "after".
fn docker_lines(args: &[&str]) -> Vec<String> {
    std::process::Command::new("docker")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------
// T112 (US5 acceptance scenario 3): both lifecycle-hook FORMS are observed distinctly.
// ---------------------------------------------------------------------------------

/// A lifecycle hook declared as an ARRAY (`["sh","-c",…]`, an argv) and one declared as an
/// OBJECT (`{"first": …, "second": …}`, named commands) are two different spellings that a
/// CLI can implement independently — and an implementation that ran only the first named
/// command of an object hook, or that treated the argv array as a sequence of shell
/// commands, would still exit 0.
///
/// So the assertion is not "the cases pass". It is that the two forms produced DISTINCT,
/// non-empty observations that could not have come from the other form: the array case wrote
/// exactly the one file its argv writes, and the object case wrote BOTH of its files, each
/// with its own command's output. Running the two cases in one test is what makes them
/// comparable — a form observed in isolation cannot show that it was distinguished.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn both_lifecycle_hook_forms_are_observed_distinctly() {
    let report_root = report_root().join("us5-lifecycle-forms");
    let cfg = tier_config(&report_root).await;

    let registry = Registry::load(&parity_root())
        .unwrap_or_else(|e| panic!("the parity case set must load: {e}"));
    let cases: Vec<_> = registry
        .cases
        .iter()
        .filter(|c| {
            c.id == "case-up-lifecycle-array-form" || c.id == "case-up-lifecycle-object-form"
        })
        .cloned()
        .collect();
    assert_eq!(
        cases.len(),
        2,
        "both lifecycle-form cases must exist; FR-047 needs the array form AND the object \
         form, and one without the other proves neither was distinguished"
    );

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, ResourceGroup::DockerShared).await);
    assert!(
        run.failures.is_empty(),
        "both lifecycle forms must be observed as declared: {}",
        run.failures.join("\n")
    );

    // Live agreement is necessary but not sufficient: it says each case matched its own
    // declaration, not that the two declarations describe DIFFERENT observations. That part
    // is structural, and it is checked against the records the runner just executed.
    let asserted_files = |case_id: &str| -> std::collections::BTreeMap<String, String> {
        let case = registry
            .cases
            .iter()
            .find(|c| c.id == case_id)
            .unwrap_or_else(|| panic!("{case_id} is in the registry"));
        case.expected
            .iter()
            .filter(|e| e.channel == "chan-file-content")
            .filter_map(|e| e.assertion.as_ref())
            .filter_map(|a| a.get("jsonSubset"))
            .filter_map(|s| s.as_object())
            .flat_map(|o| o.iter())
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    };

    let array = asserted_files("case-up-lifecycle-array-form");
    let object = asserted_files("case-up-lifecycle-object-form");

    assert_eq!(
        array.get("lifecycle-array.txt").map(String::as_str),
        Some("array-form\n"),
        "the array form is observed by the one file its argv writes: {array:?}"
    );
    assert_eq!(
        object.get("lifecycle-object-first.txt").map(String::as_str),
        Some("object-first\n"),
        "the object form is observed by its FIRST named command's file: {object:?}"
    );
    assert_eq!(
        object
            .get("lifecycle-object-second.txt")
            .map(String::as_str),
        Some("object-second\n"),
        "…and its SECOND — an implementation that stopped after one command would pass \
         every exit-code assertion, so both files must be observed: {object:?}"
    );

    // Distinct, not merely both present: neither form's observation overlaps the other's,
    // so a green run cannot be a green run of the same thing twice.
    assert!(
        array.keys().all(|k| !object.contains_key(k)),
        "the two forms must be told apart by their observations, not by the case id alone: \
         array={array:?} object={object:?}"
    );
}
