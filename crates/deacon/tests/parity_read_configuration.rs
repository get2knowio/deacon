//! Parity tests comparing deacon vs upstream devcontainer CLI for read-configuration.
//!
//! These tests are opt-in. Enable by setting `DEACON_PARITY=1` and having
//! `devcontainer` available on PATH. They are designed to assert that both CLIs
//! accomplish the same outcome: producing an equivalent effective configuration
//! for a given config file.

use serde_json::Value;
use std::path::PathBuf;

mod parity_utils;
use parity_utils::{
    normalize_config_json, repo_root, run_deacon_read_configuration,
    run_upstream_read_configuration,
};

fn config(path: &str) -> PathBuf {
    repo_root().join(path)
}

#[test]
fn parity_read_configuration_basic() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    let cfg = config("fixtures/config/basic/devcontainer.jsonc");

    let ours =
        run_deacon_read_configuration(&cfg).expect("deacon read-configuration should succeed");
    let theirs = run_upstream_read_configuration(&cfg)
        .map_err(|e| {
            eprintln!("upstream devcontainer read-configuration failed: {}", e);
            e
        })
        .expect("upstream read-configuration should succeed");

    let ours_norm: Value = normalize_config_json(&ours).expect("valid JSON from deacon");
    let theirs_norm: Value = normalize_config_json(&theirs).expect("valid JSON from upstream");

    assert_eq!(
        ours_norm, theirs_norm,
        "functional config mismatch\nours:   {}\ntheirs: {}",
        ours_norm, theirs_norm
    );
}

#[test]
fn parity_read_configuration_with_variables() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    let cfg = config("fixtures/config/with-variables/devcontainer.jsonc");

    let ours =
        run_deacon_read_configuration(&cfg).expect("deacon read-configuration should succeed");
    let theirs = run_upstream_read_configuration(&cfg)
        .map_err(|e| {
            eprintln!("upstream devcontainer read-configuration failed: {}", e);
            e
        })
        .expect("upstream read-configuration should succeed");

    let ours_norm: Value = normalize_config_json(&ours).expect("valid JSON from deacon");
    let theirs_norm: Value = normalize_config_json(&theirs).expect("valid JSON from upstream");

    assert_eq!(
        ours_norm, theirs_norm,
        "functional config mismatch\nours:   {}\ntheirs: {}",
        ours_norm, theirs_norm
    );
}
