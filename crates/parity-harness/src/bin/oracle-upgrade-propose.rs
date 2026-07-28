//! `oracle-upgrade-propose` — produce the seven-section review bundle that authorizes
//! advancing the stable oracle pin (026-continuous-conformance-certification, US6;
//! contracts/upgrade-proposal.md).
//!
//! ## It writes no pin, no disposition, and no snapshot
//!
//! There is no code path from this binary to any of them (FR-028, SC-006), and
//! `drift_hermetic` asserts that by source scan. Accepting a proposal is a human act with
//! three parts — advance the pin, re-record affected snapshots through the reviewed record
//! path, update affected dispositions — and none of them is automatable.
//!
//! ## Producing a bundle that shows heavy drift is a success
//!
//! The bundle is the deliverable. Exit `0` means it was produced; non-zero means it could
//! not be. A binary that failed when the upgrade looked expensive would be reporting an
//! opinion as an error.

use std::path::PathBuf;
use std::process::ExitCode;

use deacon_conformance::drift::{render_proposal_json, render_proposal_md};

use parity_harness::drift::proposal::{ProposalInputs, build_proposal};
use parity_harness::drift::write_drift_artifact;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let from = match flag(&args, "--from") {
        Some(v) => v,
        None => {
            eprintln!("error: --from <version> is required");
            return ExitCode::from(2);
        }
    };
    let to = match flag(&args, "--to") {
        Some(v) => v,
        None => {
            eprintln!("error: --to <version> is required");
            return ExitCode::from(2);
        }
    };

    let root = deacon_conformance::workspace_root();
    let inputs = ProposalInputs {
        from_oracle: from,
        to_oracle: to,
        repo_root: root.clone(),
        // Reference findings and newly-failing cases come from a live differential run
        // against the candidate. Absent one, the sections are emitted *investigated and
        // empty* — which is honest for a bundle prepared before the differential has been
        // run, and visibly incomplete to a reviewer who expects entries there.
        reference_findings: Vec::new(),
        newly_failing: Vec::new(),
    };

    let proposal = match build_proposal(&inputs) {
        Ok(proposal) => proposal,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    let rendered = match render_proposal_json(&proposal) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: could not serialize the upgrade proposal: {e}");
            return ExitCode::from(1);
        }
    };

    let dir = root.join("target").join("drift");
    let json_path: PathBuf = dir.join("upgrade-proposal.json");
    let md_path: PathBuf = dir.join("upgrade-proposal.md");

    // Both writes go through the checked primitive. Even here — where the targets are
    // obviously in scope — routing through it is what keeps "there is no second way in"
    // true rather than hoped for.
    if let Err(e) = write_drift_artifact(&root, &json_path, &rendered) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = write_drift_artifact(&root, &md_path, &render_proposal_md(&proposal)) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }

    eprintln!("wrote {}", json_path.display());
    eprintln!("wrote {}", md_path.display());
    eprintln!(
        "note: review all seven sections, then verify with `cargo run -p deacon-conformance \
         -- drift proposal check {}`",
        json_path.display()
    );
    ExitCode::SUCCESS
}

/// The value following `name`, if present.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
