//! T013 (US1, FR-003 / SC-012): `baseline generate` is deterministic, and the committed
//! `conformance/migration/baseline.json` matches a fresh regeneration byte-for-byte.
//!
//! The frozen-match leg (regeneration == the committed file) was the **V25 gate** and is
//! retired with it (023 T099, FR-053): once a superseded carrier is deleted, the
//! enumeration cannot reproduce the pre-migration record by construction. What remains is
//! still worth checking — the generator is deterministic, and the retained artifact is
//! well-formed.
//!
//! Hermetic: reads the repository tree, the parity registry, and the conformance
//! registry. No Docker, no network, no oracle — so it runs in every nextest profile
//! (including `dev-fast` and the Windows lane) and gates every change.

use std::path::Path;

use deacon_conformance::baseline::{generate_baseline, render};
use deacon_conformance::{default_baseline_file, workspace_root};

/// A fixed revision so determinism is measured on the enumeration, not the freeze label.
const REVISION: &str = "test-revision";

#[test]
fn regeneration_is_byte_identical_across_runs() {
    let root = workspace_root();
    let first = generate_baseline(&root, REVISION).expect("baseline enumerates cleanly");
    let second = generate_baseline(&root, REVISION).expect("baseline enumerates cleanly");

    assert_eq!(
        render(&first),
        render(&second),
        "two enumerations of an unchanged tree must render byte-identically (FR-003)"
    );
}

/// The committed baseline is an ARCHIVAL artifact: parsing and re-rendering it must
/// reproduce the file byte-for-byte, so the record cannot rot into something the loader
/// silently reinterprets.
///
/// This replaces the retired frozen-match check (V25, 023 T099). Regeneration can no
/// longer reproduce the committed file — deleting a superseded carrier removes its units
/// from the enumeration by construction — but the file's own integrity is still
/// meaningful and still checked.
#[test]
fn the_committed_baseline_round_trips_byte_for_byte() {
    let committed_path = default_baseline_file();
    let committed = std::fs::read_to_string(&committed_path).unwrap_or_else(|e| {
        panic!(
            "the committed baseline {} is retained as evidence and must remain readable: {e}",
            committed_path.display()
        )
    });
    let parsed: deacon_conformance::baseline::BaselineFile =
        serde_json::from_str(&committed).expect("committed baseline parses");
    assert_eq!(
        render(&parsed),
        committed,
        "the committed baseline must round-trip through the loader unchanged"
    );
    assert!(
        !parsed.records.is_empty(),
        "an empty baseline would make every conservation claim vacuous"
    );
}

#[test]
fn the_rendered_baseline_is_deterministic_in_shape() {
    let baseline = generate_baseline(&workspace_root(), REVISION).expect("enumerates cleanly");
    let rendered = render(&baseline);

    assert!(
        rendered.ends_with('\n'),
        "the baseline must be newline-terminated"
    );
    assert!(
        !rendered.contains('\r'),
        "the baseline must contain no CR bytes on any platform"
    );

    // No absolute paths, no machine-specific values (FR-043-style determinism).
    let root = workspace_root();
    let root_str = root.to_string_lossy().replace('\\', "/");
    assert!(
        !rendered.replace('\\', "/").contains(root_str.as_str()),
        "the baseline must contain no absolute paths"
    );

    // Records are sorted by id.
    let ids: Vec<&str> = baseline.records.iter().map(|u| u.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "records must be sorted by id");
}

#[test]
fn the_committed_baseline_carries_no_cr_bytes() {
    let path: &Path = &default_baseline_file();
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("committed baseline {} is unreadable: {e}", path.display()));
    assert!(
        !bytes.contains(&b'\r'),
        "{} contains CR bytes (line endings were translated — check the `-text` rules in \
         .gitattributes)",
        path.display()
    );
}
