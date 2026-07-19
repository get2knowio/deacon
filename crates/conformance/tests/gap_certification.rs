//! Acceptance tests for gap-driven strict certification (T022; FR-020, FR-025,
//! SC-005).
//!
//! Drives the real `conformance` binary against two fixtures:
//!
//! - `valid` — carries one gap: it is structurally valid, so `report` succeeds and
//!   shows the gap, while `certify` exits 1 listing the gap as blocking;
//! - `gap-resolved` — the same registry with the gap resolved (case added,
//!   dispositions updated, gap record removed): `certify` exits 0. Its waiver does
//!   NOT block and appears under `waived`.
//!
//! Uses `CARGO_BIN_EXE_conformance` and absolute fixture paths (via
//! `workspace_root()`), so it is CWD-independent and hermetic (no Docker/network).

use std::path::PathBuf;
use std::process::Command;

use deacon_conformance::workspace_root;

/// A fixed injected "today" so waiver-expiry (V6) never depends on the wall clock.
const TODAY: &str = "2026-07-19";

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("fixtures/conformance").join(name)
}

/// Run the `conformance` binary with the given subcommand args, returning
/// `(exit_code, stdout)`.
fn run(fixture_name: &str, args: &[&str]) -> (i32, String) {
    let bin = env!("CARGO_BIN_EXE_conformance");
    let output = Command::new(bin)
        .arg("--registry")
        .arg(fixture(fixture_name))
        .arg("--today")
        .arg(TODAY)
        .args(args)
        .output()
        .expect("conformance binary runs");
    let code = output.status.code().expect("process exited with a code");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    (code, stdout)
}

#[test]
fn gap_registry_certify_exits_1_listing_the_gap() {
    let (code, stdout) = run("valid", &["certify", "--json"]);
    assert_eq!(code, 1, "a registry with a gap must not certify (exit 1)");

    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("certify --json on stdout");
    assert_eq!(doc["certified"], false);
    let blocking = doc["blocking"].as_array().expect("blocking array");
    assert!(
        blocking
            .iter()
            .any(|b| b["kind"] == "gap" && b["id"] == "gap-readconfig-remote-user"),
        "the gap must be listed as blocking, got {blocking:?}"
    );
    // The waiver is enumerated but does NOT block certification (FR-025).
    let waived = doc["waived"].as_array().unwrap();
    assert!(
        waived.iter().any(|w| w == "wvr-readconfig-malformed-jsonc"),
        "the waiver must be enumerated under waived, got {waived:?}"
    );
}

#[test]
fn gap_registry_report_succeeds_and_shows_the_gap() {
    // `report` runs validation first; a gap is structurally valid, so it succeeds.
    let out_dir = tempfile::tempdir().expect("temp out-dir");
    let bin = env!("CARGO_BIN_EXE_conformance");
    let status = Command::new(bin)
        .arg("--registry")
        .arg(fixture("valid"))
        .arg("--today")
        .arg(TODAY)
        .arg("report")
        .arg("--out-dir")
        .arg(out_dir.path())
        .status()
        .expect("report runs");
    assert_eq!(
        status.code(),
        Some(0),
        "report on a valid+gapped registry succeeds"
    );

    let json = std::fs::read_to_string(out_dir.path().join("report.json")).expect("report.json");
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        report["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"] == "gap-readconfig-remote-user"),
        "report.json must surface the gap (FR-020)"
    );
    assert_eq!(report["summary"]["gap"], 1, "the gap counts in the summary");

    // report.md always has a Gaps section that names the gap.
    let md = std::fs::read_to_string(out_dir.path().join("report.md")).expect("report.md");
    assert!(md.contains("## Gaps"), "report.md must have a Gaps section");
    assert!(
        md.contains("gap-readconfig-remote-user"),
        "report.md Gaps section must name the gap"
    );
}

#[test]
fn resolved_gap_registry_certifies_and_waiver_is_non_blocking() {
    let (code, stdout) = run("gap-resolved", &["certify", "--json"]);
    assert_eq!(
        code, 0,
        "with the gap resolved, the registry certifies (exit 0)"
    );

    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["certified"], true);
    assert!(
        doc["blocking"].as_array().unwrap().is_empty(),
        "a resolved registry has no blocking items"
    );
    // The waiver-covered behavior does not block, but the waiver still appears.
    let waived = doc["waived"].as_array().unwrap();
    assert!(
        waived.iter().any(|w| w == "wvr-readconfig-malformed-jsonc"),
        "the waiver must still be enumerated under waived (non-blocking), got {waived:?}"
    );
}

#[test]
fn resolved_gap_report_marks_the_behavior_covered_not_gap() {
    let registry = deacon_conformance::load::Registry::load(&fixture("gap-resolved"))
        .expect("gap-resolved loads");
    let json = deacon_conformance::report::render_report_json(&registry);
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();

    // No gaps remain, and the once-gapped behavior is now a covered (non-gap) entry.
    assert_eq!(report["summary"]["gap"], 0, "no gaps after resolution");
    assert!(
        report["gaps"].as_array().unwrap().is_empty(),
        "gaps array is empty after resolution"
    );
    let resolved = report["behaviors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "bhv-readconfig-remote-user-probe")
        .expect("the resolved behavior is now in-profile covered");
    assert_ne!(
        resolved["coverage"], "gap",
        "the resolved behavior must no longer report as a gap"
    );
}
