//! Acceptance tests for waiver expiry (T014; FR-029 deterministic-test mandate).
//!
//! Exercises V6 through the injected `--today` knob at three points around the valid
//! fixture's waiver expiry (`2027-01-19`): before (valid), on the boundary
//! (`expires == today` passes), and after (expired). Both entry points are covered:
//! the library API [`validate_path`] AND the `conformance` CLI (exit codes + stdout
//! per contracts/cli.md).

use std::path::PathBuf;
use std::process::Command;

use deacon_conformance::validate::validate_path;
use deacon_conformance::workspace_root;

/// The valid fixture's sole waiver (`wvr-readconfig-malformed-jsonc`) has
/// `expires: 2027-01-19`.
const EXPIRES: &str = "2027-01-19";
const BEFORE: &str = "2027-01-18";
const AFTER: &str = "2027-01-20";

fn valid_fixture() -> PathBuf {
    workspace_root().join("fixtures/conformance/valid")
}

fn has_v6(today: &str) -> bool {
    let violations = validate_path(&valid_fixture(), today, &workspace_root())
        .expect("valid fixture root is readable");
    violations.iter().any(|v| v.code == "V6")
}

// -- Library API -------------------------------------------------------------

#[test]
fn library_before_expiry_is_valid() {
    assert!(!has_v6(BEFORE), "waiver valid before its expiry date");
    // The whole registry is clean, not merely free of V6.
    let violations = validate_path(&valid_fixture(), BEFORE, &workspace_root()).expect("readable");
    assert!(
        violations.is_empty(),
        "valid fixture clean, got: {violations:#?}"
    );
}

#[test]
fn library_boundary_expires_equals_today_passes() {
    assert!(
        !has_v6(EXPIRES),
        "expires == today must PASS (valid through the stated date)"
    );
    let violations = validate_path(&valid_fixture(), EXPIRES, &workspace_root()).expect("readable");
    assert!(
        violations.is_empty(),
        "valid fixture clean on the boundary, got: {violations:#?}"
    );
}

#[test]
fn library_after_expiry_flags_v6() {
    let violations = validate_path(&valid_fixture(), AFTER, &workspace_root()).expect("readable");
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V6" && v.record == "wvr-readconfig-malformed-jsonc"),
        "waiver must be V6 after expiry, got: {violations:#?}"
    );
    // Expiry is the ONLY thing wrong with the otherwise-valid fixture.
    assert!(
        violations.iter().all(|v| v.code == "V6"),
        "only V6 expected, got: {violations:#?}"
    );
}

// -- CLI ---------------------------------------------------------------------

/// Run `conformance validate --registry <valid> --today <today>` and return
/// (exit code, stdout).
fn run_cli(today: &str, json: bool) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_conformance"));
    cmd.arg("validate")
        .arg("--registry")
        .arg(valid_fixture())
        .arg("--today")
        .arg(today);
    if json {
        cmd.arg("--json");
    }
    let out = cmd.output().expect("conformance binary runs");
    let code = out.status.code().expect("process exited normally");
    (code, String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn cli_boundary_passes_with_exit_zero_and_empty_stdout() {
    let (code, stdout) = run_cli(EXPIRES, false);
    assert_eq!(code, 0, "boundary must exit 0, stdout={stdout:?}");
    assert!(
        stdout.trim().is_empty(),
        "text mode emits nothing on success, got {stdout:?}"
    );
}

#[test]
fn cli_after_expiry_exits_one_and_names_v6() {
    let (code, stdout) = run_cli(AFTER, false);
    assert_eq!(code, 1, "expired waiver must exit 1");
    assert!(
        stdout.contains("V6") && stdout.contains("wvr-readconfig-malformed-jsonc"),
        "text output must name the V6 waiver, got {stdout:?}"
    );
}

#[test]
fn cli_json_mode_after_expiry_is_a_single_document() {
    let (code, stdout) = run_cli(AFTER, true);
    assert_eq!(code, 1);
    let doc: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is a single JSON document");
    assert_eq!(doc["ok"], serde_json::json!(false));
    let violations = doc["violations"].as_array().expect("violations array");
    assert!(
        violations
            .iter()
            .any(|v| v["code"] == "V6" && v["record"] == "wvr-readconfig-malformed-jsonc"),
        "JSON must carry the V6 violation, got {stdout}"
    );
}

#[test]
fn cli_before_expiry_json_reports_ok_true() {
    let (code, stdout) = run_cli(BEFORE, true);
    assert_eq!(code, 0);
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("single JSON document");
    assert_eq!(doc["ok"], serde_json::json!(true));
    assert!(doc["violations"].as_array().expect("array").is_empty());
}
