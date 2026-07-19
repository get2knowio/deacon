//! Parity corpus (Tier 1b): `read-configuration --include-merged-configuration`
//! for every valid corpus case, deacon vs the pinned `@devcontainers/cli` oracle,
//! comparing the normalized `mergedConfiguration` block.
//!
//! Ported from the retired `fixtures/parity-corpus/run_tier1_merged.py` (018-
//! harden-parity-harness, research D4). Shares the `corpus_runner` skeleton with
//! `parity_corpus_tier1`, differing only in the extra `--include-merged-
//! configuration` argument and the `normalize::merged_config` entry point.
//!
//! Runs ONLY under `cargo nextest run --profile parity`.

mod corpus_runner;

use parity_harness::exec::ExecKind;
use parity_harness::normalize;

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_corpus_merged";

#[tokio::test]
async fn parity_corpus_merged() {
    corpus_runner::run_config_corpus(
        BINARY,
        ExecKind::Config,
        &["read-configuration", "--include-merged-configuration"],
        normalize::merged_config,
    )
    .await;
}
