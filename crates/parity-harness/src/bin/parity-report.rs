//! `parity-report` — the parity run-report aggregator + completeness gate
//! (research D8; FR-016, FR-018, FR-022; contracts/execution-contract.md).
//!
//! The SECOND half of `make test-parity` / `.github/workflows/parity.yml`: after
//! `cargo nextest run --profile parity` has written one fragment per live binary
//! to `<report_root>/report/`, this bin folds them into
//! `<report_root>/parity-report.json` and enforces the six gate conditions of
//! contracts/report-schema.md. It exits nonzero, enumerating every gap, unless the
//! run provably certified parity against the pinned oracle across the full
//! registry. It performs registry/fragment validation ONLY — all pass/fail
//! semantics live in the harness the test binaries used.
//!
//! `<report_root>` is `DEACON_PARITY_REPORT_DIR` when set, else
//! `<workspace_root>/target/parity` — resolved identically to the test binaries.

use std::process::ExitCode;

use parity_harness::aggregate;
use parity_harness::oracle::OraclePin;
use parity_harness::registry::ParityRegistry;
use parity_harness::{conformance_registry_root, report_root};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // The registry + pin are embedded; a malformed one is a hard, loud failure.
    let registry = match ParityRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("parity-report: cannot load registry.json: {e}");
            return ExitCode::FAILURE;
        }
    };
    let pin = match OraclePin::load() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parity-report: cannot load the oracle pin: {e}");
            return ExitCode::FAILURE;
        }
    };

    let report_root = report_root();
    // Waivers now live in the conformance registry (`conformance/registry/waivers/`),
    // consumed through `deacon-conformance` (019-conformance-registry, research D3).
    let registry_root = conformance_registry_root();

    let aggregation = match aggregate::run(&report_root, &registry_root, &registry, &pin).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("parity-report: aggregation failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let totals = &aggregation.report.totals;
    if aggregation.violations.is_empty() {
        println!(
            "parity-report: OK — {} live binaries certified against oracle {} \
             ({} cases: {} passed, {} waived); report at {}",
            aggregation.report.binaries.len(),
            aggregation.report.oracle.verified_version,
            totals.cases,
            totals.passed,
            totals.waived,
            aggregation.report_path.display(),
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "parity-report: INCOMPLETE — {} gate violation(s); parity is NOT certified:",
            aggregation.violations.len()
        );
        for violation in &aggregation.violations {
            eprintln!("  - {violation}");
        }
        eprintln!(
            "aggregated report written to {} for diagnosis.",
            aggregation.report_path.display()
        );
        ExitCode::FAILURE
    }
}
