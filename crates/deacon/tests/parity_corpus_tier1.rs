//! Parity corpus (Tier 1): `read-configuration` for every valid corpus case,
//! deacon vs the pinned `@devcontainers/cli` oracle.
//!
//! Ported from the retired `fixtures/parity-corpus/run_tier1.py` (018-harden-
//! parity-harness, research D4). ONE `#[test]` discovers every case, runs BOTH
//! CLIs via the harness, compares via the single `normalize::config` + ranked
//! diff, applies any corpus-case waivers, writes a run-report fragment, and FAILS
//! listing every offending case on any unwaived divergence, process failure,
//! normalization failure, or a discovered count below the registry minimum. There
//! is no opt-in gate and no silent skip: a missing/mismatched oracle or a missing
//! fixture FAILS the test with a cause-specific message (FR-002, FR-009, FR-024).
//!
//! Runs ONLY under `cargo nextest run --profile parity`.

mod corpus_runner;

use parity_harness::exec::ExecKind;
use parity_harness::normalize;

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_corpus_tier1";

#[tokio::test]
async fn parity_corpus_tier1() {
    corpus_runner::run_config_corpus(
        BINARY,
        ExecKind::Config,
        &["read-configuration"],
        normalize::config,
    )
    .await;
}
