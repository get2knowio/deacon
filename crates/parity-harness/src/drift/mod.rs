//! Upstream drift detection — the **live** half (026-continuous-conformance-certification,
//! US4; contracts/cli-drift.md).
//!
//! The hermetic half (records, `V33`, `drift check|report|scaffold`) lives in
//! `deacon_conformance::drift`. This module owns the only thing that needs the network:
//! looking at upstream and reporting what it currently is.
//!
//! ## What automation is allowed to write, and why the list is short
//!
//! Exactly two locations: `conformance/drift/observations.json` and `target/drift/*`
//! (FR-024a). Everything else — a pin, a disposition, a waiver, a committed snapshot — is
//! a human decision. The allow-list is enforced *before* any write is published, and an
//! out-of-scope path **aborts the run** rather than being dropped from the diff (FR-024b):
//! a silently narrowed write would misrepresent what the drift implies, which is worse
//! than not writing at all.
//!
//! ## Status reflects whether it ran, never what it found
//!
//! A scan surfacing all five drift kinds exits `0`. Only an inability to run — unreachable
//! upstream, an unresolvable pin, an unwritable artifact location, an attempted
//! out-of-scope write — is non-zero (FR-026). Upstream moving is not a defect in this
//! repository, and a lane that failed whenever it moved would be a gate on someone else's
//! release schedule.

pub mod proposal;
pub mod scan;

use std::path::Path;

use crate::HarnessError;

/// The only path prefixes drift automation may write (FR-024a).
///
/// Deliberately a constant rather than a parameter: a caller-supplied allow-list is an
/// allow-list someone can widen at the call site, and the whole point is that widening it
/// requires editing this line and explaining why.
pub const PERMITTED_WRITE_PREFIXES: &[&str] = &["conformance/drift/", "target/drift/"];

/// Reject a write target outside the allow-list (FR-024b).
///
/// Returns the offending path in the error so the abort names what it refused. `root` is
/// the repository root the path is judged relative to.
pub fn check_write_target(root: &Path, target: &Path) -> Result<(), HarnessError> {
    let relative = target.strip_prefix(root).unwrap_or(target);
    let as_posix = relative.to_string_lossy().replace('\\', "/");
    if PERMITTED_WRITE_PREFIXES
        .iter()
        .any(|prefix| as_posix.starts_with(prefix))
    {
        return Ok(());
    }
    Err(HarnessError::Report {
        cause: format!(
            "drift automation attempted to write `{as_posix}`, which is outside its \
             permitted set ({}). Aborting rather than narrowing the diff: a write that \
             silently dropped this path would misrepresent what the drift implies \
             (FR-024b). Remedy: a pin, disposition, waiver, or snapshot change is a human \
             decision — prepare it as a review artifact instead.",
            PERMITTED_WRITE_PREFIXES.join(", ")
        ),
    })
}

/// Write a drift artifact, refusing any target outside the allow-list.
///
/// The single write primitive this module exposes. Routing every write through one
/// checked function is what makes "there is no second way in" true rather than hoped for.
pub fn write_drift_artifact(
    root: &Path,
    target: &Path,
    contents: &str,
) -> Result<(), HarnessError> {
    check_write_target(root, target)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| HarnessError::Report {
            cause: format!("could not create `{}`: {e}", parent.display()),
        })?;
    }
    let temp = target.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temp, contents).map_err(|e| HarnessError::Report {
        cause: format!("could not write `{}`: {e}", temp.display()),
    })?;
    std::fs::rename(&temp, target).map_err(|e| HarnessError::Report {
        cause: format!("could not rename into `{}`: {e}", target.display()),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/repo")
    }

    #[test]
    fn the_two_permitted_locations_are_accepted() {
        for permitted in [
            "conformance/drift/observations.json",
            "target/drift/scan.json",
            "target/drift/upgrade-proposal.md",
        ] {
            assert!(check_write_target(&root(), &root().join(permitted)).is_ok());
        }
    }

    #[test]
    fn a_pin_a_record_and_a_snapshot_are_all_refused() {
        for forbidden in [
            "conformance/registry/revisions.json",
            "conformance/registry/waivers/wvr-x.json",
            "conformance/registry/cases/up.json",
            "conformance/snapshots/linux-x86_64/case-a/provenance.json",
            "fixtures/parity-corpus/oracle.json",
            "conformance/discovery/canary.json",
        ] {
            let err =
                check_write_target(&root(), &root().join(forbidden)).expect_err("must refuse");
            assert!(
                err.to_string().contains(forbidden),
                "the abort must name the attempted path, got: {err}"
            );
        }
    }

    #[test]
    fn a_near_miss_outside_the_drift_root_is_still_refused() {
        // `conformance/drifted/` is not `conformance/drift/`. Prefix matching is exact by
        // path segment in intent; this asserts the near miss does not slip through.
        assert!(
            check_write_target(&root(), &root().join("conformance/registry/drift.json")).is_err()
        );
    }

    #[test]
    fn writing_leaves_no_temp_file_and_refuses_out_of_scope_targets() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ok = dir.path().join("target/drift/scan.json");
        write_drift_artifact(dir.path(), &ok, "{}\n").expect("permitted write");
        assert_eq!(std::fs::read_to_string(&ok).expect("read"), "{}\n");
        let leftovers: Vec<_> = std::fs::read_dir(ok.parent().expect("parent"))
            .expect("read dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(leftovers, vec!["scan.json".to_string()]);

        let bad = dir.path().join("conformance/registry/revisions.json");
        assert!(write_drift_artifact(dir.path(), &bad, "{}").is_err());
        assert!(!bad.exists(), "a refused write must leave nothing behind");
    }
}
