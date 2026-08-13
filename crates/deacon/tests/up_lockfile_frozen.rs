//! Integration tests for the CLI feature-overlay controls and lockfile/frozen validation.
//!
//! Tests for User Story 2 (Deterministic feature selection) from
//! specs/007-up-build-parity:
//! - --ignore-additional-features enforcement (formerly --skip-feature-auto-mapping, #498)
//! - lockfile/frozen mode validation
//!
//! These tests verify the fail-fast validation behavior without requiring Docker,
//! by testing the CLI argument parsing and validation logic through temporary
//! file fixtures.

use deacon::commands::up::UpArgs;
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
use deacon_core::lockfile::{
    Lockfile, LockfileFeature, LockfileValidationResult, get_lockfile_path, read_lockfile,
    validate_lockfile_against_config, write_lockfile,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a minimal devcontainer.json with optional features
fn create_devcontainer_config(dir: &Path, features: Option<serde_json::Value>) {
    let config = serde_json::json!({
        "name": "Test Container",
        "image": "alpine:3.18",
        "features": features.unwrap_or(serde_json::json!({}))
    });

    let config_dir = dir.join(".devcontainer");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("devcontainer.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

/// Helper to create a lockfile with specified features
fn create_lockfile(config_path: &Path, feature_ids: &[&str]) {
    let lockfile_path = get_lockfile_path(config_path);

    let mut features = HashMap::new();
    for id in feature_ids {
        features.insert(
            id.to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: format!("{}@sha256:{}", id, "a".repeat(64)),
                integrity: format!("sha256:{}", "a".repeat(64)),
                depends_on: None,
            },
        );
    }

    let lockfile = Lockfile { features };
    block_on_async(write_lockfile(&lockfile_path, &lockfile, true)).unwrap();
}

/// Bridge async lockfile helpers into the existing synchronous `#[test]` setup
/// in this file. Each test gets its own tiny current-thread runtime so we don't
/// take a runtime dependency on test scaffolding.
fn block_on_async<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime for test")
        .block_on(fut)
}

// =============================================================================
// Feature-overlay control tests (--ignore-additional-features)
// =============================================================================

/// Test: `--ignore-additional-features` alongside `--additional-features` is the
/// combination that means something — the overlay is dropped, not rejected.
///
/// This pairing used to live on `--skip-feature-auto-mapping` (007 FR-004), a name
/// borrowed from the reference CLI where it gates nothing at all (#498).
#[test]
fn test_ignore_additional_features_with_additional_features_drops_overlay() {
    let args = UpArgs {
        ignore_additional_features: true,
        additional_features: Some(r#"{"ghcr.io/devcontainers/features/node:1":{}}"#.to_string()),
        ..Default::default()
    };

    assert!(
        args.ignore_additional_features && args.additional_features.is_some(),
        "Test setup: both flags should be set"
    );

    // Both flags together are legal — `up` logs that the overlay was dropped rather
    // than erroring, so the effective feature set is the configuration's alone.
    let merge_config = FeatureMergeConfig::new(
        args.additional_features.clone(),
        false,
        None,
        args.ignore_additional_features,
    );
    let config_features = serde_json::json!({});
    let merged = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
    assert!(
        merged.as_object().unwrap().is_empty(),
        "the CLI overlay must be dropped when ignore_additional_features is set"
    );
}

/// Test: `--ignore-additional-features` without `--additional-features` is a no-op.
#[test]
fn test_ignore_additional_features_without_additional_features_passes() {
    let args = UpArgs {
        ignore_additional_features: true,
        additional_features: None,
        ..Default::default()
    };

    assert!(
        args.ignore_additional_features,
        "ignore_additional_features should be enabled"
    );
    assert!(
        args.additional_features.is_none(),
        "additional_features should be None"
    );
}

/// Test: `ignore_additional_features` defaults to false.
#[test]
fn test_ignore_additional_features_defaults_to_false() {
    let args = UpArgs::default();
    assert!(
        !args.ignore_additional_features,
        "ignore_additional_features should default to false"
    );
}

/// Test: `ignore_additional_features` blocks CLI features via FeatureMerger.
///
/// When it is true, additional CLI features should NOT be added to the config
/// features - only explicitly declared config features remain.
#[test]
fn test_ignore_additional_features_blocks_cli_features() {
    // Config with one feature declared
    let config_features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {"version": "18"}
    });

    // Create merge config with ignore_additional_features enabled and additional features
    let merge_config = FeatureMergeConfig::new(
        Some(r#"{"ghcr.io/devcontainers/features/go:1": {}}"#.to_string()),
        false, // prefer_cli_features
        None,  // feature_install_order
        true,  // ignore_additional_features - this is the key flag
    );

    // Merge features - CLI features should be ignored
    let merged = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();

    // Verify only config features remain (CLI features blocked)
    let merged_obj = merged.as_object().unwrap();
    assert!(
        merged_obj.contains_key("ghcr.io/devcontainers/features/node:1"),
        "Config feature should be preserved"
    );
    assert!(
        !merged_obj.contains_key("ghcr.io/devcontainers/features/go:1"),
        "CLI feature should be blocked when ignore_additional_features is enabled"
    );
    assert_eq!(
        merged_obj.len(),
        1,
        "Only one feature should remain when CLI features are blocked"
    );
}

/// Test: ignore_additional_features with no CLI features preserves config features.
///
/// When ignore_additional_features is enabled and no additional CLI features are provided,
/// the config features should be preserved exactly as declared.
#[test]
fn test_ignore_additional_features_with_no_cli_features() {
    // Config with multiple features declared
    let config_features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {"version": "18"},
        "ghcr.io/devcontainers/features/python:1": {"version": "3.11"}
    });

    // Create merge config with ignore_additional_features but NO additional features
    let merge_config = FeatureMergeConfig::new(
        None,  // additional_features - none
        false, // prefer_cli_features
        None,  // feature_install_order
        true,  // ignore_additional_features
    );

    // Merge features
    let merged = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();

    // Verify all config features are preserved
    let merged_obj = merged.as_object().unwrap();
    assert!(
        merged_obj.contains_key("ghcr.io/devcontainers/features/node:1"),
        "Node feature should be preserved"
    );
    assert!(
        merged_obj.contains_key("ghcr.io/devcontainers/features/python:1"),
        "Python feature should be preserved"
    );
    assert_eq!(
        merged_obj.len(),
        2,
        "Both config features should be preserved"
    );
}

// =============================================================================
// Frozen Lockfile Tests
// =============================================================================

/// Test: frozen mode with missing lockfile should fail.
///
/// Per spec (FR-005): Up MUST enforce lockfile and frozen modes so that
/// any deviation from the locked feature set halts execution.
/// Missing lockfile in frozen mode is a deviation that must fail.
#[test]
fn test_frozen_lockfile_missing_fails() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with features but NO lockfile
    let features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {}
    });
    create_devcontainer_config(temp_dir.path(), Some(features.clone()));

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");
    let lockfile_path = get_lockfile_path(&config_path);

    // Verify lockfile does NOT exist
    assert!(
        !lockfile_path.exists(),
        "Test setup: lockfile should not exist"
    );

    // Read lockfile (will return None since it doesn't exist)
    let lockfile = block_on_async(read_lockfile(&lockfile_path)).unwrap();
    assert!(lockfile.is_none(), "Lockfile should be None when missing");

    // Validate against config - should return Missing result
    let validation_result =
        validate_lockfile_against_config(lockfile.as_ref(), &features, &lockfile_path);

    // Verify the validation result is Missing
    match &validation_result {
        LockfileValidationResult::Missing { expected_path } => {
            assert_eq!(
                expected_path, &lockfile_path,
                "Missing result should contain expected lockfile path"
            );
        }
        other => panic!(
            "Expected Missing result, got: {:?}. Frozen mode requires lockfile to exist.",
            other
        ),
    }

    // Verify error message content matches upstream-aligned format
    // ("Lockfile does not exist." / "--frozen-lockfile" — see lockfile.rs format_error)
    let error_msg = validation_result.format_error();
    assert!(
        error_msg.contains("Lockfile does not exist."),
        "Error message should match upstream missing-lockfile string. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("--frozen-lockfile"),
        "Error message should mention the graduated flag. Got: {}",
        error_msg
    );
}

/// Test: frozen mode with mismatched features (config has more than lockfile) should fail.
///
/// Per spec: features declared in config but missing from lockfile is a mismatch.
#[test]
fn test_frozen_lockfile_mismatch_fails() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with TWO features
    let features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {},
        "ghcr.io/devcontainers/features/go:1": {}
    });
    create_devcontainer_config(temp_dir.path(), Some(features.clone()));

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");

    // Create lockfile with only ONE feature (missing go)
    create_lockfile(&config_path, &["ghcr.io/devcontainers/features/node:1"]);

    let lockfile_path = get_lockfile_path(&config_path);
    assert!(lockfile_path.exists(), "Lockfile should exist");

    // Read and validate lockfile
    let lockfile = block_on_async(read_lockfile(&lockfile_path))
        .unwrap()
        .unwrap();
    let validation_result =
        validate_lockfile_against_config(Some(&lockfile), &features, &lockfile_path);

    // Verify the validation result indicates missing feature
    match &validation_result {
        LockfileValidationResult::MissingFromLockfile { features } => {
            assert!(
                features.contains(&"ghcr.io/devcontainers/features/go:1".to_string()),
                "Missing features should include 'go:1'. Got: {:?}",
                features
            );
        }
        other => panic!("Expected MissingFromLockfile result, got: {:?}", other),
    }

    // Verify error message content
    let error_msg = validation_result.format_error();
    assert!(
        error_msg.contains("Features declared in config but missing from lockfile"),
        "Error message should describe mismatch direction. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("go:1"),
        "Error message should list missing feature. Got: {}",
        error_msg
    );
}

/// Test: lockfile mode (non-frozen) with mismatch warns but continues.
///
/// When lockfile validation is enabled but NOT frozen mode,
/// mismatches should emit a warning but not block execution.
#[test]
fn test_lockfile_mismatch_warns_continues() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with ONE feature
    let features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {}
    });
    create_devcontainer_config(temp_dir.path(), Some(features.clone()));

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");

    // Create lockfile with EXTRA feature (lockfile has more than config)
    create_lockfile(
        &config_path,
        &[
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/go:1",
        ],
    );

    let lockfile_path = get_lockfile_path(&config_path);
    let lockfile = block_on_async(read_lockfile(&lockfile_path))
        .unwrap()
        .unwrap();

    // Validate lockfile against config
    let validation_result =
        validate_lockfile_against_config(Some(&lockfile), &features, &lockfile_path);

    // Should be ExtraInLockfile (not a match)
    assert!(
        !validation_result.is_matched(),
        "Validation should not match when lockfile has extra features"
    );

    match &validation_result {
        LockfileValidationResult::ExtraInLockfile { features } => {
            assert!(
                features.contains(&"ghcr.io/devcontainers/features/go:1".to_string()),
                "Extra features should include 'go:1'. Got: {:?}",
                features
            );
        }
        other => panic!("Expected ExtraInLockfile result, got: {:?}", other),
    }

    // In non-frozen mode, this would produce a warning but continue.
    // The format_error() provides the warning message that would be logged.
    let warning_msg = validation_result.format_error();
    assert!(
        warning_msg.contains("Features in lockfile but not declared in config"),
        "Warning should describe the mismatch. Got: {}",
        warning_msg
    );
}

/// Test: frozen mode with valid lockfile should pass validation.
///
/// When the lockfile exists and features match the config, validation passes.
#[test]
fn test_frozen_mode_with_valid_lockfile_passes() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with a feature
    let features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {}
    });
    create_devcontainer_config(temp_dir.path(), Some(features.clone()));

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");

    // Create matching lockfile
    create_lockfile(&config_path, &["ghcr.io/devcontainers/features/node:1"]);

    let lockfile_path = get_lockfile_path(&config_path);

    // Verify lockfile exists
    assert!(lockfile_path.exists(), "Lockfile should exist");

    // Read and validate
    let lockfile = block_on_async(read_lockfile(&lockfile_path))
        .unwrap()
        .unwrap();
    let validation_result =
        validate_lockfile_against_config(Some(&lockfile), &features, &lockfile_path);

    // Should match
    assert!(
        validation_result.is_matched(),
        "Validation should pass when lockfile matches config features"
    );
    assert_eq!(
        validation_result,
        LockfileValidationResult::Matched,
        "Result should be Matched variant"
    );
}

/// Test: frozen mode with extra features in lockfile (not in config) should fail.
///
/// Per spec: features in lockfile but not declared in config is also a mismatch.
#[test]
fn test_frozen_mode_with_lockfile_features_not_in_config_fails() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with ONE feature
    let features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {}
    });
    create_devcontainer_config(temp_dir.path(), Some(features.clone()));

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");

    // Create lockfile with TWO features (extra go)
    create_lockfile(
        &config_path,
        &[
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/go:1",
        ],
    );

    let lockfile_path = get_lockfile_path(&config_path);
    let lockfile = block_on_async(read_lockfile(&lockfile_path))
        .unwrap()
        .unwrap();

    let validation_result =
        validate_lockfile_against_config(Some(&lockfile), &features, &lockfile_path);

    // Should fail with ExtraInLockfile
    assert!(
        !validation_result.is_matched(),
        "Validation should fail when lockfile has extra features"
    );

    match &validation_result {
        LockfileValidationResult::ExtraInLockfile { features } => {
            assert_eq!(features.len(), 1, "Should have exactly one extra feature");
            assert!(
                features.contains(&"ghcr.io/devcontainers/features/go:1".to_string()),
                "Extra feature should be go:1"
            );
        }
        other => panic!("Expected ExtraInLockfile, got: {:?}", other),
    }

    let error_msg = validation_result.format_error();
    assert!(
        error_msg.contains("Features in lockfile but not declared in config"),
        "Error should describe the mismatch direction. Got: {}",
        error_msg
    );
}

/// Test: frozen mode with empty features in both config and lockfile should pass.
#[test]
fn test_frozen_mode_with_no_features_passes() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with NO features
    let features = serde_json::json!({});
    create_devcontainer_config(temp_dir.path(), None);

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");

    // Create empty lockfile
    create_lockfile(&config_path, &[]);

    let lockfile_path = get_lockfile_path(&config_path);
    assert!(lockfile_path.exists(), "Lockfile should exist");

    // Read and validate
    let lockfile = block_on_async(read_lockfile(&lockfile_path))
        .unwrap()
        .unwrap();
    assert!(
        lockfile.features.is_empty(),
        "Lockfile should have no features"
    );

    let validation_result =
        validate_lockfile_against_config(Some(&lockfile), &features, &lockfile_path);

    assert!(
        validation_result.is_matched(),
        "Empty config and empty lockfile should match"
    );
}

// (Deleted: test_experimental_frozen_lockfile_defaults_to_false — the
// `--experimental-*` deprecation aliases have been removed; only the
// graduated `--frozen-lockfile` / `--no-lockfile` flags remain.)

// =============================================================================
// Lockfile Path Derivation Tests
// =============================================================================

/// Test: lockfile path is derived correctly from config path.
#[test]
fn test_lockfile_path_derivation() {
    let config_path = Path::new(".devcontainer/devcontainer.json");
    let lockfile_path = get_lockfile_path(config_path);
    assert_eq!(
        lockfile_path,
        Path::new(".devcontainer/devcontainer-lock.json")
    );
}

/// Test: lockfile path for hidden config file.
#[test]
fn test_lockfile_path_derivation_hidden_config() {
    let config_path = Path::new(".devcontainer/.devcontainer.json");
    let lockfile_path = get_lockfile_path(config_path);
    assert_eq!(
        lockfile_path,
        Path::new(".devcontainer/.devcontainer-lock.json")
    );
}

// =============================================================================
// Flag-surface tests
// =============================================================================

/// Test: `--skip-feature-auto-mapping` stays ACCEPTED on every subcommand that used
/// to carry it, and `--ignore-additional-features` is the flag that now carries the
/// behavior (#498).
///
/// The reference CLI accepts its identically-named flag on all of these and does
/// nothing with it, so deacon accepting-and-ignoring is the parity-preserving shape;
/// rejecting it would break callers that pass it because the reference's docs mention
/// it. What must NOT survive is deacon giving it a meaning of its own.
#[test]
fn test_both_flags_parse_on_every_feature_subcommand() {
    use clap::Parser;
    use deacon::cli::Cli;

    for subcommand in ["up", "build", "read-configuration"] {
        Cli::try_parse_from(["deacon", subcommand, "--skip-feature-auto-mapping"])
            .unwrap_or_else(|e| panic!("{subcommand} must still accept the compat flag: {e}"));
        Cli::try_parse_from(["deacon", subcommand, "--ignore-additional-features"])
            .unwrap_or_else(|e| panic!("{subcommand} must accept the renamed flag: {e}"));
    }
}

/// Test: the compat flag is inert — it does not reach the merge behavior.
///
/// `UpArgs` no longer has a field for it at all, which is the structural proof: the
/// only path from the command line to `FeatureMergeConfig` is
/// `--ignore-additional-features`.
#[test]
fn test_compat_flag_does_not_drop_the_cli_overlay() {
    let config_features = serde_json::json!({});
    // What `--skip-feature-auto-mapping` alone now produces: the default (false).
    let merge_config = FeatureMergeConfig::new(
        Some(r#"{"ghcr.io/devcontainers/features/go:1": {}}"#.to_string()),
        false,
        None,
        UpArgs::default().ignore_additional_features,
    );
    let merged = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
    assert!(
        merged
            .as_object()
            .unwrap()
            .contains_key("ghcr.io/devcontainers/features/go:1"),
        "the CLI overlay must survive when only the inert compat flag was passed"
    );
}

/// Test: expected error message for missing lockfile in frozen mode.
#[test]
fn test_frozen_missing_lockfile_error_message_content() {
    // Create the Missing result
    let result = LockfileValidationResult::Missing {
        expected_path: std::path::PathBuf::from("/path/to/devcontainer-lock.json"),
    };

    let error_msg = result.format_error();

    // Verify message matches upstream-aligned format
    assert!(
        error_msg.contains("Lockfile does not exist."),
        "Error should use upstream missing-lockfile string. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("/path/to/devcontainer-lock.json"),
        "Error should include the expected path. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("--frozen-lockfile"),
        "Error should reference the graduated flag. Got: {}",
        error_msg
    );
}

/// Test: expected error message for lockfile mismatch (missing from lockfile).
#[test]
fn test_frozen_mismatch_missing_from_lockfile_error_message_content() {
    let result = LockfileValidationResult::MissingFromLockfile {
        features: vec!["ghcr.io/devcontainers/features/node:1".to_string()],
    };

    let error_msg = result.format_error();

    assert!(
        error_msg.contains("Lockfile does not match."),
        "Error should use upstream mismatch string. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("Features declared in config but missing from lockfile"),
        "Error should describe mismatch direction. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("node:1"),
        "Error should list the missing feature. Got: {}",
        error_msg
    );
}

/// Test: expected error message for lockfile mismatch (extra in lockfile).
#[test]
fn test_frozen_mismatch_extra_in_lockfile_error_message_content() {
    let result = LockfileValidationResult::ExtraInLockfile {
        features: vec!["ghcr.io/devcontainers/features/stale:1".to_string()],
    };

    let error_msg = result.format_error();

    assert!(
        error_msg.contains("Lockfile does not match."),
        "Error should use upstream mismatch string. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("Features in lockfile but not declared in config"),
        "Error should describe mismatch direction. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("stale:1"),
        "Error should list the extra feature. Got: {}",
        error_msg
    );
}

/// Test: expected error message for bidirectional mismatch.
#[test]
fn test_frozen_mismatch_bidirectional_error_message_content() {
    let result = LockfileValidationResult::Mismatch {
        missing_from_lockfile: vec!["ghcr.io/devcontainers/features/new:1".to_string()],
        extra_in_lockfile: vec!["ghcr.io/devcontainers/features/old:1".to_string()],
    };

    let error_msg = result.format_error();

    assert!(
        error_msg.contains("Lockfile does not match."),
        "Error should use upstream mismatch string. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("new:1"),
        "Error should list the missing feature. Got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("old:1"),
        "Error should list the extra feature. Got: {}",
        error_msg
    );
}

// =============================================================================
// UpArgs Struct Tests
// =============================================================================

/// Test: UpArgs fields for feature control exist and have correct defaults.
#[test]
fn test_up_args_feature_control_fields() {
    let args = UpArgs::default();

    // ignore_additional_features should exist and default to false
    assert!(!args.ignore_additional_features);

    // frozen_lockfile should default to false
    assert!(!args.frozen_lockfile);

    // additional_features should exist and default to None
    assert!(args.additional_features.is_none());
}

/// Test: UpArgs can be constructed with all feature control options.
#[test]
fn test_up_args_with_all_feature_options() {
    let args = UpArgs {
        ignore_additional_features: true,
        frozen_lockfile: true,
        additional_features: None,
        prefer_cli_features: false,
        feature_install_order: Some("feature-a,feature-b".to_string()),
        ..Default::default()
    };

    assert!(args.ignore_additional_features);
    assert!(args.frozen_lockfile);
    assert!(args.additional_features.is_none());
    assert!(!args.prefer_cli_features);
    assert_eq!(
        args.feature_install_order,
        Some("feature-a,feature-b".to_string())
    );
}

// =============================================================================
// Combined Scenario Tests
// =============================================================================

/// Test: Both frozen lockfile AND --ignore-additional-features can be enabled together.
///
/// These are independent controls that work together for maximum determinism.
#[test]
fn test_frozen_lockfile_with_ignore_additional_features() {
    let args = UpArgs {
        ignore_additional_features: true,
        frozen_lockfile: true,
        additional_features: None,
        ..Default::default()
    };

    // Both should be enabled without conflict
    assert!(
        args.ignore_additional_features && args.frozen_lockfile,
        "Both frozen lockfile and ignore-additional-features should be enableable together"
    );
}

/// Test: Lockfile with multiple features validates correctly.
#[test]
fn test_lockfile_validation_multiple_features() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with multiple features
    let features = serde_json::json!({
        "ghcr.io/devcontainers/features/node:1": {"version": "18"},
        "ghcr.io/devcontainers/features/python:1": {"version": "3.11"},
        "ghcr.io/devcontainers/features/go:1": {"version": "1.21"}
    });
    create_devcontainer_config(temp_dir.path(), Some(features.clone()));

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");

    // Create matching lockfile with all features
    create_lockfile(
        &config_path,
        &[
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/python:1",
            "ghcr.io/devcontainers/features/go:1",
        ],
    );

    let lockfile_path = get_lockfile_path(&config_path);
    let lockfile = block_on_async(read_lockfile(&lockfile_path))
        .unwrap()
        .unwrap();

    let validation_result =
        validate_lockfile_against_config(Some(&lockfile), &features, &lockfile_path);

    assert!(
        validation_result.is_matched(),
        "All features matching should result in Matched"
    );
}

// =============================================================================
// #569: the pre-build refusal must see Features supplied by --additional-features
// =============================================================================

/// `up --frozen-lockfile` must refuse BEFORE any daemon work even when the
/// Features arrive via `--additional-features` rather than the configuration.
///
/// The reference consults `--frozen-lockfile` only inside `writeLockfile`
/// (`mQ`), reached from the single caller `generateFeaturesConfig` (`UQ`),
/// whose early return tests `userFeaturesToArray` (`xQ`) — and `xQ` takes the
/// UNION of the configuration's Features and `additionalFeatures`:
///
/// ```js
/// function xQ(A, e) {
///   if (!Object.keys(A.features || {}).length && !Object.keys(e || {}).length) return;
///   …
/// }
/// ```
///
/// deacon's gate read `config.features()` alone, so this exact invocation
/// resolved the Feature and BUILT the Feature-extended image before refusing —
/// measured at oracle 0.87.0, where the reference emits zero `Start: Run:
/// docker …` lines for the same run.
///
/// **The docker-less `PATH` is the assertion, not a convenience.** The exit
/// code and the error document already agreed with the reference before the
/// fix; what differed was only *when* the refusal happened, and no parity
/// channel observes an intermediate image left on the daemon. Emptying `PATH`
/// turns that ordering into something a test can see: any daemon access fails
/// loudly with `Docker is not installed or not accessible`, so reaching the
/// lockfile message proves nothing touched Docker first. Watched to fail —
/// before the fix this test received exactly that Docker error.
#[test]
fn frozen_lockfile_refuses_additional_features_before_touching_docker() {
    let temp_dir = TempDir::new().unwrap();

    // A configuration that declares NO Features of its own — the whole point.
    create_devcontainer_config(temp_dir.path(), None);

    let config_path = temp_dir.path().join(".devcontainer/devcontainer.json");
    let lockfile_path = get_lockfile_path(&config_path);
    assert!(
        !lockfile_path.exists(),
        "test setup: no lockfile may exist on disk"
    );

    let output = assert_cmd::Command::cargo_bin("deacon")
        .expect("deacon binary")
        .env("PATH", "/nonexistent-empty-dir")
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--frozen-lockfile")
        .arg("--additional-features")
        .arg(r#"{"ghcr.io/devcontainers/features/git:1.3.2":{}}"#)
        .output()
        .expect("run deacon up");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Lockfile does not exist."),
        "the frozen gate must fire on the CLI-supplied Feature. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("Docker is not installed"),
        "reaching Docker at all means the gate ran too late (#569). stdout: {stdout}"
    );
    assert_eq!(output.status.code(), Some(1), "refusal exits 1");
    assert!(
        !lockfile_path.exists(),
        "the refusal must not create the lockfile it refused over"
    );
}
