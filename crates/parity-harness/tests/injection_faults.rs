//! Guards for the injected-regression harness (024-deterministic-conformance-coverage
//! US6, T126–T131; contract regression-harness.md, research Decision 5).
//!
//! This is the suite that watches the watchman. `coverage-regressions` is the only thing
//! standing between a dead observable channel and a trusted green conformance run, so the
//! ways IT can be wrong are exactly the ways a green suite becomes worthless:
//!
//! | # | Failure mode | Guard |
//! |---|---|---|
//! | T126 | a perturbation that fails the run but names the wrong thing | the verdict names the CHANNEL under test |
//! | T127 | an inert channel reported as a warning | an inert channel makes the run exit non-zero (FR-067) |
//! | T128 | a perturbation left applied | the tree is unmodified after a run, including after an UNWIND (FR-066) |
//! | T129 | a flaky classification | the same inputs classify identically twice (FR-069) |
//! | T130 | a DEAD observer reported `detected` | injecting upstream of the observer reports it `inert` (FR-065b) |
//! | T131 | an ordinary run perturbing its own evidence | the capability is unreachable outside the one bin (FR-070) |
//!
//! Hermetic: synthetic evidence, temp directories, and a source scan. No Docker, no
//! oracle, no network.

use std::path::{Path, PathBuf};

use deacon_conformance::model::{
    CHAN_EXIT_CODE, CHAN_FILESYSTEM, CHAN_STDOUT, OBSERVED_CHANNELS, Operation,
};
use deacon_conformance::regression::{RegressionFile, RegressionRecord};
use parity_harness::compare::verdict_spec_expectation;
use parity_harness::evidence::{Outcome, RawChannelEvidence};
use parity_harness::inject::{
    RecordResult, RegressionHarness, RegressionReport, RegressionVerdict, activate, detects,
    intercept, perturb_source,
};
use parity_harness::normalize::{normalize_channel, tokens_for_channel};
use parity_harness::observe::{ChannelObserver, ProcessOutcome, RunContext, observer_for};
use parity_harness::{HarnessError, workspace_root};

// ---------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------

/// Take out the process-level injection capability. Every test here that perturbs anything
/// needs it; the test that proves an ordinary process CANNOT get it lives in the library's
/// own unit tests (`inject::tests`), which run in a process that never declares it.
fn capability() -> RegressionHarness {
    RegressionHarness::declare()
}

fn record(json: &str) -> RegressionRecord {
    let file: RegressionFile =
        serde_json::from_str(&format!(r#"{{"records":[{json}]}}"#)).expect("record loads");
    file.records.into_iter().next().expect("exactly one record")
}

/// A `RunContext` carrying one captured process result — the raw artifact the CLI-process
/// observers read.
fn ctx_with(workspace: &Path, stdout: &str, exit: i32) -> RunContext {
    let mut ctx = RunContext::new(workspace.to_path_buf());
    ctx.record_outcome(
        "op",
        ProcessOutcome {
            exit_code: Some(exit),
            success: exit == 0,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            failure_phase: None,
        },
    );
    ctx
}

fn op() -> Operation {
    Operation {
        id: "op".to_string(),
        subcommand: "read-configuration".to_string(),
        ..Operation::default()
    }
}

/// Capture + normalize + evaluate one channel exactly as the runner does: the REAL
/// observer for the channel, the single normalizer, then the declared assertion.
fn verdict(ctx: &RunContext, channel: &str, assertion: serde_json::Value) -> Outcome {
    let observer = observer_for(channel).expect("the channel has an observer");
    let raw = observer.capture(ctx, &op()).expect("capture succeeds");
    let tokens = tokens_for_channel(channel, &ctx.workspace);
    let normalized = normalize_channel(channel, &raw, &tokens, parity_harness::exec::Side::Deacon);
    verdict_spec_expectation(channel, &normalized, &assertion)
        .expect("the assertion is well formed")
        .outcome
}

/// The exit-code record used throughout: deacon's rejection of malformed input is masked
/// as success.
fn exit_code_record() -> RegressionRecord {
    record(
        r#"{
          "id": "reg-exit-code-failure-masked",
          "channel": "chan-exit-code",
          "target": "process-result",
          "perturbation": { "kind": "set-exit-code", "exitCode": 0 },
          "expectedDetectingCases": ["case-errors-decl-malformed-json"]
        }"#,
    )
}

// ---------------------------------------------------------------------------------
// T126 — an injected regression fails the run, and the failure names the CHANNEL
// ---------------------------------------------------------------------------------

/// The point of naming the channel is attribution. A run that merely goes red proves that
/// *something* broke; the record's claim is that THIS channel noticed, and only a verdict
/// carrying the channel id can support it.
#[test]
fn an_injected_regression_fails_the_run_and_the_failure_names_the_channel() {
    let _capability = capability();
    let temp = tempfile::tempdir().expect("tempdir");
    let rec = exit_code_record();

    // Clean: deacon rejected the input, so `nonZero` holds.
    let mut ctx = ctx_with(temp.path(), "", 1);
    let before = verdict(&ctx, CHAN_EXIT_CODE, serde_json::json!({ "nonZero": true }));
    assert_eq!(before, Outcome::Agree, "the baseline must be clean");

    // Perturbed at the evidence source: the captured status becomes 0.
    perturb_source(&mut ctx, &rec).expect("the perturbation applies");
    let after = verdict(&ctx, CHAN_EXIT_CODE, serde_json::json!({ "nonZero": true }));
    assert_eq!(
        after,
        Outcome::Diverge,
        "masking a failure as success must turn the channel red"
    );
    assert!(detects(Some(before), Some(after)));

    // …and the report attributes it to the channel under test, by name.
    let report = RegressionReport::build(vec![(
        rec.channel.clone(),
        RecordResult {
            id: rec.id.clone(),
            detected_by: vec!["case-errors-decl-malformed-json".to_string()],
            notes: vec![],
        },
    )]);
    assert_eq!(report.channels[0].channel, CHAN_EXIT_CODE);
    assert_eq!(report.channels[0].verdict, RegressionVerdict::Detected);
    assert_eq!(report.exit_status(), 0);
}

// ---------------------------------------------------------------------------------
// T127 — an inert channel FAILS the run (FR-067)
// ---------------------------------------------------------------------------------

/// This is the guard that gives the whole feature its meaning. An inert channel must be a
/// FAILURE, never a warning: a warning is a thing a green pipeline scrolls past, and the
/// state it describes — a channel nobody can make fail — retroactively empties every
/// result that rested on it.
#[test]
fn a_channel_with_no_detecting_regression_is_inert_and_fails_the_run() {
    let inert = RegressionReport::build(vec![(
        CHAN_STDOUT.to_string(),
        RecordResult {
            id: "reg-stdout-appended-marker".to_string(),
            detected_by: vec![], // nothing detected it
            notes: vec!["case-x: chan-stdout stayed agree under the perturbation".to_string()],
        },
    )]);
    assert_eq!(inert.channels[0].verdict, RegressionVerdict::Inert);
    assert_eq!(inert.inert_count, 1);
    assert_eq!(inert.inert_channels(), vec![CHAN_STDOUT]);
    assert_eq!(
        inert.exit_status(),
        1,
        "an inert channel must fail the run, not warn (FR-067)"
    );

    // The same record WITH a detection is the only thing that clears it.
    let live = RegressionReport::build(vec![(
        CHAN_STDOUT.to_string(),
        RecordResult {
            id: "reg-stdout-appended-marker".to_string(),
            detected_by: vec!["case-x".to_string()],
            notes: vec![],
        },
    )]);
    assert_eq!(live.inert_count, 0);
    assert_eq!(live.exit_status(), 0);

    // And a mixed report fails on the inert channel alone.
    let mixed = RegressionReport::build(vec![
        (
            CHAN_STDOUT.to_string(),
            RecordResult {
                id: "reg-a".to_string(),
                detected_by: vec![],
                notes: vec![],
            },
        ),
        (
            CHAN_EXIT_CODE.to_string(),
            RecordResult {
                id: "reg-b".to_string(),
                detected_by: vec!["case-y".to_string()],
                notes: vec![],
            },
        ),
    ]);
    assert_eq!(mixed.exit_status(), 1);
    assert_eq!(mixed.inert_channels(), vec![CHAN_STDOUT]);
}

/// The committed record set must cover EVERY channel the harness can observe, or the
/// acceptance run would report `inertCount: 0` while saying nothing about the channels it
/// never exercised. (V30 enforces the same thing against `channels.json`; this asserts it
/// against the observable set the harness actually has observers for.)
#[test]
fn every_observed_channel_has_a_committed_regression_record() {
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("the committed conformance registry loads");
    let covered: Vec<&str> = registry
        .regressions
        .iter()
        .map(|r| r.channel.as_str())
        .collect();
    for channel in OBSERVED_CHANNELS {
        assert!(
            covered.contains(channel),
            "channel {channel:?} has an observer but no `reg-` record — a run would report \
             inertCount 0 while never exercising it"
        );
    }
}

// ---------------------------------------------------------------------------------
// T128 — the tree is unmodified after a run, including after an unwind (FR-066)
// ---------------------------------------------------------------------------------

/// Every file under `root`, as `(relative path, bytes)` — the whole-tree fingerprint the
/// before/after comparison is made on.
fn snapshot_tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                out.push((rel, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn scaffolded_workspace() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("applied/.devcontainer");
    std::fs::create_dir_all(&dir).expect("create fixture dirs");
    std::fs::write(
        dir.join("devcontainer.json"),
        "{\n\t\"name\": \"conformance-minimal\"\n}\n",
    )
    .expect("write fixture");
    std::fs::write(temp.path().join("untouched.txt"), b"neighbour").expect("write neighbour");
    temp
}

fn remove_path_record() -> RegressionRecord {
    record(
        r#"{
          "id": "reg-filesystem-scaffold-missing",
          "channel": "chan-filesystem",
          "target": "workspace-file",
          "perturbation": { "kind": "remove-path", "path": "applied/.devcontainer/devcontainer.json" },
          "expectedDetectingCases": ["case-templates-apply-option-substituted"]
        }"#,
    )
}

#[test]
fn a_filesystem_perturbation_is_applied_then_fully_reverted() {
    let _capability = capability();
    let temp = scaffolded_workspace();
    let before = snapshot_tree(temp.path());

    let rec = remove_path_record();
    let mut ctx = RunContext::new(temp.path().to_path_buf());
    ctx.fs_allowlist = vec!["applied/.devcontainer/devcontainer.json".to_string()];
    {
        let guard = activate(&rec).expect("armed");
        intercept(&mut ctx).expect("applied at the evidence-source boundary");

        // The perturbation really landed: the observer sees the file as ABSENT, which is
        // the difference the filesystem channel exists to catch.
        let outcome = verdict(
            &ctx,
            CHAN_FILESYSTEM,
            serde_json::json!({ "exists": "applied/.devcontainer/devcontainer.json" }),
        );
        assert_eq!(outcome, Outcome::Diverge);
        assert_eq!(guard.applied_count(), 1);
        guard.finish().expect("revert succeeds");
    }

    assert_eq!(
        snapshot_tree(temp.path()),
        before,
        "the tree must be byte-identical after the run (FR-066)"
    );
}

/// The unwind path is the one that matters: an early `?`, a failing assertion, or a
/// panicking case must not be able to leave the tree perturbed. `Drop` — not the happy
/// path — is what carries that guarantee, exactly as `DockerWorkspace`'s does.
#[test]
fn the_tree_is_restored_even_when_the_run_panics() {
    let _capability = capability();
    let temp = scaffolded_workspace();
    let before = snapshot_tree(temp.path());
    let root = temp.path().to_path_buf();
    let rec = remove_path_record();

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = activate(&rec).expect("armed");
        let mut ctx = RunContext::new(root.clone());
        intercept(&mut ctx).expect("applied");
        assert!(
            !root
                .join("applied/.devcontainer/devcontainer.json")
                .exists(),
            "the perturbation is applied before the panic"
        );
        panic!("a case blew up mid-run");
    }))
    .is_err();
    assert!(panicked, "the closure must actually have unwound");

    assert_eq!(
        snapshot_tree(temp.path()),
        before,
        "the RAII guard must revert on unwind, not only on the happy path (FR-066)"
    );
}

// ---------------------------------------------------------------------------------
// T129 — the classification is reproducible (FR-069)
// ---------------------------------------------------------------------------------

/// A classification that varies run to run is worse than no classification: `detected`
/// would be a coin flip, and an `inert` result could always be re-rolled away.
#[test]
fn the_detected_inert_classification_is_identical_across_two_runs() {
    let _capability = capability();
    let rec = exit_code_record();

    let classify_once = || {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut ctx = ctx_with(temp.path(), "", 1);
        let before = verdict(&ctx, CHAN_EXIT_CODE, serde_json::json!({ "nonZero": true }));
        perturb_source(&mut ctx, &rec).expect("applies");
        let after = verdict(&ctx, CHAN_EXIT_CODE, serde_json::json!({ "nonZero": true }));
        let detected = detects(Some(before), Some(after));
        RegressionReport::build(vec![(
            rec.channel.clone(),
            RecordResult {
                id: rec.id.clone(),
                detected_by: if detected {
                    vec!["case-errors-decl-malformed-json".to_string()]
                } else {
                    vec![]
                },
                notes: vec![],
            },
        )])
        .render()
        .expect("renders")
    };

    let first = classify_once();
    let second = classify_once();
    assert_eq!(
        first, second,
        "the same inputs must produce a byte-identical report (FR-069)"
    );
    assert!(first.contains("\"verdict\": \"detected\""), "{first}");
}

// ---------------------------------------------------------------------------------
// T130 — a DEAD observer is reported INERT (research Decision 5, FR-065b)
// ---------------------------------------------------------------------------------

/// An observer that ignores its input entirely and always returns the same evidence.
///
/// This is the failure the injection point exists to catch, constructed deliberately
/// rather than hoped-not-to-exist: a channel whose observer stopped looking at the system
/// still *has* an observer, still returns evidence, and still compares equal — so the
/// channel reports green forever while proving nothing.
struct DeadObserver;

impl ChannelObserver for DeadObserver {
    fn channel(&self) -> &'static str {
        CHAN_EXIT_CODE
    }

    fn capture(
        &self,
        _ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        // Constant. Never reads `_ctx`, so it cannot see any perturbation of it.
        Ok(RawChannelEvidence {
            channel: CHAN_EXIT_CODE.to_string(),
            operation: op.id.clone(),
            present: true,
            value: serde_json::json!(1),
        })
    }
}

#[test]
fn a_dead_observer_is_reported_inert_rather_than_falsely_detected() {
    let _capability = capability();
    let temp = tempfile::tempdir().expect("tempdir");
    let rec = exit_code_record();
    let assertion = serde_json::json!({ "nonZero": true });

    // --- the LIVE observer: the perturbation is visible, so the channel is detected ----
    let mut live_ctx = ctx_with(temp.path(), "", 1);
    let live_before = verdict(&live_ctx, CHAN_EXIT_CODE, assertion.clone());
    perturb_source(&mut live_ctx, &rec).expect("applies");
    let live_after = verdict(&live_ctx, CHAN_EXIT_CODE, assertion.clone());
    assert!(
        detects(Some(live_before), Some(live_after)),
        "a live observer must see the perturbed source"
    );

    // --- the DEAD observer: same perturbation, same evidence, so NOT detected ---------
    let mut dead_ctx = ctx_with(temp.path(), "", 1);
    let dead_evidence = |ctx: &RunContext| {
        let raw = DeadObserver.capture(ctx, &op()).expect("capture");
        let tokens = tokens_for_channel(CHAN_EXIT_CODE, &ctx.workspace);
        let normalized = normalize_channel(
            CHAN_EXIT_CODE,
            &raw,
            &tokens,
            parity_harness::exec::Side::Deacon,
        );
        verdict_spec_expectation(CHAN_EXIT_CODE, &normalized, &assertion)
            .expect("well formed")
            .outcome
    };
    let dead_before = dead_evidence(&dead_ctx);
    perturb_source(&mut dead_ctx, &rec).expect("applies to the SOURCE all the same");
    let dead_after = dead_evidence(&dead_ctx);

    assert_eq!(
        dead_before, dead_after,
        "a dead observer returns the same evidence whatever the source says"
    );
    assert!(
        !detects(Some(dead_before), Some(dead_after)),
        "a dead observer must NOT be reported as detecting anything"
    );

    // …and that is what the report says: the channel is INERT, and the run fails.
    let report = RegressionReport::build(vec![(
        rec.channel.clone(),
        RecordResult {
            id: rec.id.clone(),
            detected_by: vec![],
            notes: vec!["the observer ignored the perturbed source".to_string()],
        },
    )]);
    assert_eq!(report.channels[0].verdict, RegressionVerdict::Inert);
    assert_eq!(report.exit_status(), 1);

    // The proof that the injection point is UPSTREAM: the raw source really did change,
    // even though the dead observer's output did not. Had the harness perturbed the
    // observer's RETURN value instead, this dead observer would have reported `detected`.
    assert_eq!(
        dead_ctx.outcome("op").expect("outcome").exit_code,
        Some(0),
        "the evidence SOURCE was perturbed; only the observer failed to look"
    );
}

// ---------------------------------------------------------------------------------
// T131 — an ordinary conformance run cannot apply a regression (FR-070)
// ---------------------------------------------------------------------------------

/// Every source file that could reach the injector, and the one file allowed to.
const CAPABILITY_OWNER: &str = "coverage-regressions.rs";

/// Files whose call graph is the ORDINARY conformance run: the two driver test binaries
/// and the harness modules they go through.
const ORDINARY_RUN_SOURCES: &[&str] = &[
    "crates/deacon/tests/parity_conformance_runner.rs",
    "crates/deacon/tests/parity_conformance_docker.rs",
    "crates/parity-harness/src/runner.rs",
    "crates/parity-harness/src/driver.rs",
    "crates/parity-harness/src/oracle_type.rs",
];

/// Recursively collect `.rs` files under `dir`.
fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// STRUCTURAL, not conventional: the capability that enables injection is taken out in
/// exactly one program, so no other program's call graph can reach the injector at all.
///
/// The alternative — "the drivers just don't do that" — is a comment, and a comment does
/// not survive the next person wiring a convenience helper. Enabling injection requires
/// `RegressionHarness::declare`, and this asserts that call exists in one file only.
#[test]
fn only_the_coverage_regressions_bin_can_enable_injection() {
    let root = workspace_root();
    let mut sources = Vec::new();
    rust_sources(&root.join("crates/parity-harness/src"), &mut sources);
    rust_sources(&root.join("crates/parity-harness/tests"), &mut sources);
    rust_sources(&root.join("crates/deacon/tests"), &mut sources);
    rust_sources(&root.join("crates/deacon/src"), &mut sources);
    rust_sources(&root.join("crates/conformance/src"), &mut sources);
    assert!(!sources.is_empty(), "the source scan found no files");

    let mut declaring: Vec<String> = Vec::new();
    for path in &sources {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        // Skip this guard's own helper and the documentation that names the call.
        if text.contains("RegressionHarness::declare()") {
            declaring.push(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    declaring.sort();
    declaring.dedup();
    assert_eq!(
        declaring,
        vec![
            CAPABILITY_OWNER.to_string(),
            "injection_faults.rs".to_string()
        ],
        "the injection capability must be taken out by the `coverage-regressions` bin and \
         by this guard alone — any other program that declares it can perturb its own \
         evidence (FR-070)"
    );
}

/// The ordinary run's call graph must not even be able to ARM a regression: no driver, and
/// none of the harness modules a driver goes through, may name `activate` / `perturb_source`.
/// The single permitted contact point is `inject::intercept`, which is a no-op without an
/// armed regression.
#[test]
fn the_ordinary_run_can_only_reach_the_inert_hook() {
    let root = workspace_root();
    for rel in ORDINARY_RUN_SOURCES {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        for forbidden in [
            "inject::activate",
            "inject::perturb_source",
            "RegressionHarness",
        ] {
            assert!(
                !text.contains(forbidden),
                "{rel} references {forbidden:?}; the ordinary conformance run must not be \
                 able to arm or apply a regression (FR-070)"
            );
        }
    }
    // …and the runner DOES call the hook, so the boundary exists where it is documented to.
    let runner = std::fs::read_to_string(root.join("crates/parity-harness/src/runner.rs"))
        .expect("runner.rs is readable");
    assert!(
        runner.contains("inject::intercept(&mut ctx)"),
        "the evidence-source-boundary hook must be wired in the runner, between capture \
         and observation"
    );
}

/// The runtime half: reaching the hook with NOTHING armed perturbs nothing. This is the
/// state every ordinary run is in — the drivers call `intercept` on every case and it must
/// leave the captured evidence byte-identical.
#[test]
fn the_hook_perturbs_nothing_when_no_regression_is_armed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ctx = ctx_with(temp.path(), "result", 1);
    let before_calls = parity_harness::inject::intercept_count();

    intercept(&mut ctx).expect("the unarmed hook is a no-op, never an error");

    assert!(
        parity_harness::inject::intercept_count() > before_calls,
        "the hook was actually reached — otherwise this proves nothing"
    );
    let outcome = ctx.outcome("op").expect("outcome");
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), "result");
}

/// Arming refuses outright in a process without the capability. Asserted here on the
/// error TYPE via a record the fixture registry never sees, so a future refactor that
/// turns the refusal into a silent no-op fails loudly.
#[test]
fn arming_without_the_capability_is_refused_by_type() {
    // This test binary DOES hold the capability (other tests take it out), so the refusal
    // itself is asserted in `inject::tests`, which runs in a process that never declares
    // it. What is asserted here is that the refusal is a distinct, named error rather than
    // a boolean — the shape a caller cannot accidentally ignore.
    let err = HarnessError::InjectionForbidden {
        record: "reg-x".to_string(),
    };
    let rendered = err.to_string();
    assert!(rendered.contains("reg-x"), "{rendered}");
    assert!(
        rendered.contains("coverage-regressions"),
        "the remedy must name the one program that may inject: {rendered}"
    );
}
