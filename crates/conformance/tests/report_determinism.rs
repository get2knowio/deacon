//! Acceptance tests for report determinism (T023; SC-004, FR-024).
//!
//! Generates `report.json`/`report.md` for the `valid` fixture twice — into
//! different `--out-dir`s and with different injected `--today` values — and asserts
//! the two outputs are byte-identical. The report is a pure function of registry
//! content (research Decision 7): the out-dir and today's date must NOT leak into
//! the artifacts. Drives the real `conformance` binary; hermetic, CWD-independent.

use std::path::{Path, PathBuf};
use std::process::Command;

use deacon_conformance::workspace_root;

fn fixture() -> PathBuf {
    workspace_root().join("fixtures/conformance/valid")
}

/// Run `conformance report` for the valid fixture into `out_dir` with the given
/// injected `--today`, asserting success.
fn generate(out_dir: &Path, today: &str) {
    let bin = env!("CARGO_BIN_EXE_conformance");
    let status = Command::new(bin)
        .arg("--registry")
        .arg(fixture())
        .arg("--today")
        .arg(today)
        .arg("report")
        .arg("--out-dir")
        .arg(out_dir)
        .status()
        .expect("report runs");
    assert_eq!(
        status.code(),
        Some(0),
        "report must succeed for the valid fixture"
    );
}

fn read(out_dir: &Path, name: &str) -> String {
    std::fs::read_to_string(out_dir.join(name))
        .unwrap_or_else(|e| panic!("reading {name} from {}: {e}", out_dir.display()))
}

#[test]
fn reports_are_byte_identical_across_out_dirs_and_injected_today() {
    let a = tempfile::tempdir().expect("temp dir a");
    let b = tempfile::tempdir().expect("temp dir b");

    // Different out-dirs AND different injected --today values.
    generate(a.path(), "2026-07-19");
    generate(b.path(), "2027-01-01");

    assert_eq!(
        read(a.path(), "report.json"),
        read(b.path(), "report.json"),
        "report.json must be byte-identical regardless of out-dir or --today"
    );
    assert_eq!(
        read(a.path(), "report.md"),
        read(b.path(), "report.md"),
        "report.md must be byte-identical regardless of out-dir or --today"
    );
}

#[test]
fn reports_are_byte_identical_for_repeated_same_today_runs() {
    let a = tempfile::tempdir().expect("temp dir a");
    let b = tempfile::tempdir().expect("temp dir b");

    // Same injected --today, distinct out-dirs → still byte-identical.
    generate(a.path(), "2026-07-19");
    generate(b.path(), "2026-07-19");

    assert_eq!(read(a.path(), "report.json"), read(b.path(), "report.json"));
    assert_eq!(read(a.path(), "report.md"), read(b.path(), "report.md"));
}

#[test]
fn report_artifacts_contain_no_environment_data() {
    let dir = tempfile::tempdir().expect("temp dir");
    generate(dir.path(), "2026-07-19");

    let json = read(dir.path(), "report.json");
    // No absolute paths, no injected date, no out-dir path leaks (SC-004).
    assert!(
        !json.contains("/workspaces/"),
        "no absolute paths in report.json"
    );
    assert!(!json.contains("/tmp/"), "no out-dir path in report.json");
    assert!(
        !json.contains("2026-07-19"),
        "the injected --today must not leak"
    );
    assert!(
        !json.contains(&dir.path().display().to_string()),
        "the out-dir path must not leak into report.json"
    );
}

/// The valid fixture ships no sibling inventory, so the constraint-inventory section
/// (020-schema-constraint-inventory, T028) is present-but-zeroed in both artifacts.
#[test]
fn fixture_report_has_present_but_zeroed_inventory_section() {
    let dir = tempfile::tempdir().expect("temp dir");
    generate(dir.path(), "2026-07-19");

    let json = read(dir.path(), "report.json");
    let doc: serde_json::Value = serde_json::from_str(&json).expect("report.json parses");
    let inv = &doc["inventory"];
    assert_eq!(
        inv["totalUnits"], 0,
        "no committed inventory for the fixture"
    );
    assert_eq!(inv["revision"], "");
    // All 15 kinds present-but-zero → stable shape even with no inventory.
    assert_eq!(
        inv["unitsByKind"].as_object().map(|m| m.len()),
        Some(15),
        "all 15 ConstraintKinds are always present in unitsByKind"
    );
    assert!(inv["unclassified"].as_array().unwrap().is_empty());
    assert!(inv["stale"].as_array().unwrap().is_empty());

    let md = read(dir.path(), "report.md");
    assert!(
        md.contains("## Constraint inventory"),
        "report.md must carry the constraint-inventory section"
    );
    assert!(
        md.contains("No committed inventory."),
        "empty-inventory registries render an explicit none state"
    );
}

/// Run `conformance report` against the real repository registry (default
/// `--registry`), which picks up its sibling committed inventory + classifications.
fn generate_real(out_dir: &Path) {
    let bin = env!("CARGO_BIN_EXE_conformance");
    let status = Command::new(bin)
        .arg("report")
        .arg("--out-dir")
        .arg(out_dir)
        .status()
        .expect("report runs");
    assert_eq!(
        status.code(),
        Some(0),
        "report must succeed for the real registry"
    );
}

/// The real registry's committed inventory (609 units, base 403 / feature 206) is
/// summarized in the report's inventory section, fully classified (no unclassified,
/// no stale), and the section is byte-deterministic across runs (SC-004, T028).
#[test]
fn real_registry_report_has_populated_inventory_section() {
    let a = tempfile::tempdir().expect("temp dir a");
    let b = tempfile::tempdir().expect("temp dir b");
    generate_real(a.path());
    generate_real(b.path());

    // Byte-deterministic across independent runs.
    assert_eq!(read(a.path(), "report.json"), read(b.path(), "report.json"));
    assert_eq!(read(a.path(), "report.md"), read(b.path(), "report.md"));

    let doc: serde_json::Value =
        serde_json::from_str(&read(a.path(), "report.json")).expect("report.json parses");
    let inv = &doc["inventory"];
    assert_eq!(inv["revision"], "rev-schema-113500f4");
    assert_eq!(inv["totalUnits"], 609);
    assert_eq!(inv["unitsByDocument"]["base"], 403);
    assert_eq!(inv["unitsByDocument"]["feature"], 206);
    // Disposition tallies sum to the total-unit count (every unit classified exactly once).
    let d = &inv["dispositions"];
    let sum = d["behaviorMapped"].as_u64().unwrap()
        + d["nonTestable"].as_u64().unwrap()
        + d["notApplicable"].as_u64().unwrap();
    assert_eq!(sum, 609, "every unit carries exactly one disposition");
    assert!(
        inv["unclassified"].as_array().unwrap().is_empty(),
        "100% of units classified (SC-003)"
    );
    assert!(
        inv["stale"].as_array().unwrap().is_empty(),
        "no stale classification records"
    );
}
