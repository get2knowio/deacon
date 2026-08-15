//! Fail when an open issue asserting a deacon-vs-reference difference has no
//! `parity/SPEC_STATUS.md` row.
//!
//! The decision lives in [`parity_harness::registry::check_ledger_covers_issues`], which is
//! a pure function with hermetic tests. This binary is only the plumbing that gets a live
//! issue list to it — it is the one place in the parity harness that needs the network, and
//! keeping it OUT of the library is what leaves the hermetic lane's no-network promise
//! intact.
//!
//! Usage (see `.github/workflows/ci.yml`):
//!
//! ```text
//! gh issue list --state open --limit 200 \
//!   --json number,title,labels \
//!   --jq '[.[] | {number, title, labels: [.labels[].name]}]' \
//!   | cargo run -q -p parity-harness --bin ledger-issue-coverage
//! ```
//!
//! stdin is the issue list because `gh` is the tool that already has the credentials and
//! the pagination; re-implementing either against the REST API in Rust would add a token
//! path and a dependency to restate what one shell line does.
//!
//! Exit 0 = every obligation discharged. Exit 1 = at least one is not, with each named on
//! stderr. Exit 2 = the check could not run (unreadable ledger, unparseable input), which is
//! deliberately NOT folded into exit 1: "the guard failed" and "the guard found something"
//! are different facts and a CI log should not have to guess which one it is reading.

use std::io::Read as _;
use std::process::ExitCode;

use parity_harness::parity_root;
use parity_harness::registry::{LedgerIssue, check_ledger_covers_issues};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let allow_empty = args.iter().any(|a| a == "--allow-empty");

    // The ledger path is an optional argument rather than a constant so the guard can be
    // WATCHED FAILING, which the campaign requires of every check before it counts. The
    // only honest demonstration is a historical replay — `git show <sha>:parity/SPEC_STATUS.md`
    // from a day the ledger was known to be incomplete — and that is impossible against a
    // hardcoded path. Defaults to this checkout's ledger, so CI passes no argument.
    let ledger_path = match args.iter().find(|a| !a.starts_with("--")) {
        Some(path) => std::path::PathBuf::from(path),
        None => parity_root().join("SPEC_STATUS.md"),
    };
    let markdown = match std::fs::read_to_string(&ledger_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("could not read {}: {err}", ledger_path.display());
            return ExitCode::from(2);
        }
    };

    let mut raw = String::new();
    if let Err(err) = std::io::stdin().read_to_string(&mut raw) {
        eprintln!("could not read the issue list from stdin: {err}");
        return ExitCode::from(2);
    }

    // An EMPTY stdin is a failure, not a vacuous pass. `gh` returning nothing means the
    // query broke, and a check that reports "no obligations" when it was handed no issues
    // is the exact shape of the defect it exists to prevent.
    let issues: Vec<LedgerIssue> = match serde_json::from_str(raw.trim()) {
        Ok(issues) => issues,
        Err(err) => {
            eprintln!(
                "could not parse the issue list as JSON ({err}). Expected the output of \
                 `gh issue list --json number,title,labels --jq '[.[] | {{number, title, \
                 labels: [.labels[].name]}}]'`; got {} byte(s).",
                raw.trim().len()
            );
            return ExitCode::from(2);
        }
    };

    // An EMPTY issue list is treated as a broken query, not as a clean repository. A guard
    // that reports "every obligation discharged" after checking nothing is the precise
    // shape of the defect this exists to prevent — and a `gh` invocation that loses its
    // token, its repo or its `--jq` filter returns `[]` while exiting 0, so success here
    // would be indistinguishable from having run. `--allow-empty` is the deliberate
    // override for a repository that genuinely has no open issues.
    if issues.is_empty() && !allow_empty {
        eprintln!(
            "the issue list is empty, which is treated as a FAILED QUERY rather than a clean \
             repository: a check that passes without checking anything cannot be told apart \
             from one that ran. Verify the `gh issue list` invocation (repo, auth, --jq), or \
             pass --allow-empty if this repository really has no open issues."
        );
        return ExitCode::from(2);
    }

    let problems = check_ledger_covers_issues(&issues, &markdown);
    if problems.is_empty() {
        println!(
            "ledger issue coverage: {} open issue(s) checked, every obligation discharged",
            issues.len()
        );
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "ledger issue coverage: {} problem(s) across {} open issue(s)",
        problems.len(),
        issues.len()
    );
    for problem in &problems {
        eprintln!("  - {problem}");
    }
    ExitCode::FAILURE
}
