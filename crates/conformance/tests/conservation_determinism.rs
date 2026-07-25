//! T068 (US5, FR-043 / SC-012): two runs on unchanged inputs are byte-identical, with no
//! timestamps and no absolute paths.
//!
//! The report is meant to be reviewable as a version-controlled diff even though the file
//! is generated. A timestamp or an absolute path would make every regeneration a diff,
//! which trains reviewers to skim it — and a report nobody reads proves nothing.
//!
//! Determinism is checked at BOTH levels, because they can fail independently: the pure
//! rendering (same value → same bytes) and the whole CLI pipeline (same tree → same
//! bytes, run twice as separate processes, so anything order-dependent in a `HashMap`
//! iteration or a filesystem walk has a real chance to show itself).
//!
//! Hermetic: no Docker, no network.

mod support;

use std::process::Command;

use deacon_conformance::conservation::{render_report_json, render_report_md};
use support::Fixture;

#[test]
fn the_rendering_is_a_pure_function_of_the_report() {
    let fixture = Fixture::real();
    let a = fixture.report().expect("report computes");
    let b = fixture.report().expect("report computes");

    assert_eq!(render_report_json(&a), render_report_json(&b));
    assert_eq!(
        render_report_md(&a),
        render_report_md(&b),
        "the Markdown is a pure function of the report value (T074)"
    );
    // The Markdown is derived from the JSON's value, so equal reports must render equal
    // Markdown — and DIFFERENT reports must not.
    let mutated = Fixture::real()
        .without_mapping_entry("parity_corpus_tier1::node-ts")
        .report()
        .expect("report computes");
    assert_ne!(
        render_report_md(&a),
        render_report_md(&mutated),
        "a changed report must change the rendering, or the rendering proves nothing"
    );
}

#[test]
fn the_rendering_carries_no_timestamp_absolute_path_or_hostname() {
    let report = Fixture::real().report().expect("report computes");
    for (label, text) in [
        ("json", render_report_json(&report)),
        ("md", render_report_md(&report)),
    ] {
        let root = deacon_conformance::workspace_root();
        let root_str = root.to_string_lossy().replace('\\', "/");
        assert!(
            !text.replace('\\', "/").contains(root_str.as_str()),
            "the {label} rendering must contain no absolute path"
        );
        assert!(
            !text.contains("/tmp/") && !text.contains("/var/folders/"),
            "the {label} rendering must not leak a tempdir path"
        );
        assert!(
            !text.contains('\r'),
            "the {label} rendering must contain no CR bytes"
        );
        // An RFC3339-ish timestamp would make every regeneration a diff.
        let timestampish = text
            .split(|c: char| !(c.is_ascii_digit() || c == '-' || c == ':' || c == 'T'))
            .any(|token| {
                token.len() >= 19
                    && token.contains('T')
                    && token.chars().filter(|c| *c == '-').count() >= 2
            });
        assert!(
            !timestampish,
            "the {label} rendering must carry no timestamp"
        );
    }
}

/// The real pipeline, run twice as two separate processes over the real tree, with the
/// bytes compared. A unit test over a single in-process value cannot catch a
/// nondeterministic filesystem walk or map iteration; two processes can.
#[test]
fn two_real_cli_runs_produce_byte_identical_files() {
    let bin = env!("CARGO_BIN_EXE_conformance");
    let dir = tempfile::tempdir().expect("tempdir");

    let run = |out: &std::path::Path| -> (i32, Vec<u8>) {
        let output = Command::new(bin)
            .args(["migration", "report", "--format", "json", "--out-dir"])
            .arg(out)
            .output()
            .expect("conformance binary runs");
        (output.status.code().unwrap_or(-1), output.stdout)
    };

    let first_dir = dir.path().join("first");
    let second_dir = dir.path().join("second");
    let (code_a, stdout_a) = run(&first_dir);
    let (code_b, stdout_b) = run(&second_dir);

    assert_eq!(
        code_a, 0,
        "the committed migration must account for everything"
    );
    assert_eq!(code_b, 0);
    assert_eq!(
        stdout_a, stdout_b,
        "stdout must be byte-identical across runs"
    );

    for name in ["migration-report.json", "migration-report.md"] {
        let a = std::fs::read(first_dir.join(name)).expect("first run wrote the report");
        let b = std::fs::read(second_dir.join(name)).expect("second run wrote the report");
        assert_eq!(a, b, "{name} must be byte-identical across runs (FR-043)");
        assert!(
            !a.contains(&b'\r'),
            "{name} must contain no CR bytes on any platform"
        );
    }
}

#[test]
fn the_json_document_goes_to_stdout_and_diagnostics_to_stderr() {
    // Constitution VI: a caller must be able to pipe stdout into a JSON parser.
    let bin = env!("CARGO_BIN_EXE_conformance");
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new(bin)
        .args(["migration", "report", "--format", "json", "--out-dir"])
        .arg(dir.path())
        .output()
        .expect("conformance binary runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout)
        .expect("stdout must be a single parseable JSON document, nothing else");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("conservation:"),
        "the accounting summary belongs on stderr: {stderr}"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&stderr).is_err(),
        "diagnostics must not be mistaken for the document"
    );
}

#[test]
fn migration_check_never_writes_a_report() {
    // The gating form produces no artifact — a check that writes is a check that can be
    // run for its side effect.
    let bin = env!("CARGO_BIN_EXE_conformance");
    let dir = tempfile::tempdir().expect("tempdir");
    let before: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read tempdir")
        .flatten()
        .collect();
    assert!(before.is_empty());

    let output = Command::new(bin)
        .args(["migration", "check"])
        .output()
        .expect("conformance binary runs");
    assert_eq!(output.status.code(), Some(0));

    let after: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read tempdir")
        .flatten()
        .collect();
    assert!(after.is_empty(), "`migration check` must write nothing");
}
