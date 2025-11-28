//! Unit tests for up command flag parsing and validation
//!
//! Tests validation rules from specs/001-up-gap-spec/contracts/up.md:
//! - workspace_folder OR id_label required
//! - workspace_folder OR override_config required
//! - mount regex validation
//! - remote_env format validation
//! - terminal dimensions pairing

use deacon::commands::shared::TerminalDimensions;
use deacon::commands::up::{NormalizedMount, NormalizedRemoteEnv, UpResult};

#[test]
fn test_normalized_mount_validation_bind_basic() {
    let result = NormalizedMount::parse("type=bind,source=/host/path,target=/container/path");
    assert!(result.is_ok());
    let mount = result.unwrap();
    assert_eq!(mount.source, "/host/path");
    assert_eq!(mount.target, "/container/path");
    assert!(!mount.read_only);
}

#[test]
fn test_normalized_mount_validation_volume_with_external() {
    let result = NormalizedMount::parse("type=volume,source=myvolume,target=/data,external=true");
    assert!(result.is_ok());
    let mount = result.unwrap();
    assert_eq!(mount.source, "myvolume");
    assert_eq!(mount.target, "/data");
    assert!(mount.read_only);
}

#[test]
fn test_normalized_mount_validation_missing_target() {
    let result = NormalizedMount::parse("type=bind,source=/tmp");
    assert!(result.is_err());
}

#[test]
fn test_normalized_mount_validation_invalid_type() {
    let result = NormalizedMount::parse("type=invalid,source=/tmp,target=/data");
    assert!(result.is_err());
}

#[test]
fn test_normalized_mount_validation_missing_source() {
    let result = NormalizedMount::parse("type=bind,target=/data");
    assert!(result.is_err());
}

#[test]
fn test_normalized_remote_env_validation_basic() {
    let result = NormalizedRemoteEnv::parse("FOO=bar");
    assert!(result.is_ok());
    let env = result.unwrap();
    assert_eq!(env.name, "FOO");
    assert_eq!(env.value, "bar");
}

#[test]
fn test_normalized_remote_env_validation_with_equals_in_value() {
    let result = NormalizedRemoteEnv::parse("DATABASE_URL=postgres://user:pass@host/db");
    assert!(result.is_ok());
    let env = result.unwrap();
    assert_eq!(env.name, "DATABASE_URL");
    assert_eq!(env.value, "postgres://user:pass@host/db");
}

#[test]
fn test_normalized_remote_env_validation_empty_value() {
    let result = NormalizedRemoteEnv::parse("EMPTY=");
    assert!(result.is_ok());
    let env = result.unwrap();
    assert_eq!(env.name, "EMPTY");
    assert_eq!(env.value, "");
}

#[test]
fn test_normalized_remote_env_validation_missing_equals() {
    let result = NormalizedRemoteEnv::parse("INVALID");
    assert!(result.is_err());
}

#[test]
fn test_terminal_dimensions_both_specified() {
    let result = TerminalDimensions::new(Some(80), Some(24));
    assert!(result.is_ok());
    let dims = result.unwrap();
    assert!(dims.is_some());
    let dims = dims.unwrap();
    assert_eq!(dims.columns, 80);
    assert_eq!(dims.rows, 24);
}

#[test]
fn test_terminal_dimensions_neither_specified() {
    let result = TerminalDimensions::new(None, None);
    assert!(result.is_ok());
    let dims = result.unwrap();
    assert!(dims.is_none());
}

#[test]
fn test_terminal_dimensions_only_columns_fails() {
    let result = TerminalDimensions::new(Some(80), None);
    assert!(result.is_err());
}

#[test]
fn test_terminal_dimensions_only_rows_fails() {
    let result = TerminalDimensions::new(None, Some(24));
    assert!(result.is_err());
}

// JSON output serialization tests

#[test]
fn test_up_result_success_basic_serialization() {
    let result = UpResult::success(
        "container123".to_string(),
        "vscode".to_string(),
        "/workspaces/myproject".to_string(),
    );

    assert!(result.is_success());
    assert!(!result.is_error());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "success");
    assert_eq!(json["containerId"], "container123");
    assert_eq!(json["remoteUser"], "vscode");
    assert_eq!(json["remoteWorkspaceFolder"], "/workspaces/myproject");
    assert!(json.get("composeProjectName").is_none());
    assert!(json.get("configuration").is_none());
    assert!(json.get("mergedConfiguration").is_none());
}

#[test]
fn test_up_result_success_with_compose_project() {
    let result = UpResult::success(
        "container123".to_string(),
        "vscode".to_string(),
        "/workspaces/myproject".to_string(),
    )
    .with_compose_project_name("myproject".to_string());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "success");
    assert_eq!(json["composeProjectName"], "myproject");
}

#[test]
fn test_up_result_success_with_configuration() {
    let config = serde_json::json!({
        "name": "Test Config",
        "image": "node:18"
    });

    let result = UpResult::success(
        "container123".to_string(),
        "vscode".to_string(),
        "/workspaces/myproject".to_string(),
    )
    .with_configuration(config.clone());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "success");
    assert_eq!(json["configuration"], config);
}

#[test]
fn test_up_result_success_with_merged_configuration() {
    let merged_config = serde_json::json!({
        "name": "Test Config",
        "image": "node:18",
        "features": {
            "ghcr.io/devcontainers/features/node:1": "18"
        }
    });

    let result = UpResult::success(
        "container123".to_string(),
        "vscode".to_string(),
        "/workspaces/myproject".to_string(),
    )
    .with_merged_configuration(merged_config.clone());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "success");
    assert_eq!(json["mergedConfiguration"], merged_config);
}

#[test]
fn test_up_result_error_basic_serialization() {
    let result = UpResult::error(
        "Validation failed".to_string(),
        "Invalid mount format: missing target".to_string(),
    );

    assert!(!result.is_success());
    assert!(result.is_error());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["message"], "Validation failed");
    assert_eq!(json["description"], "Invalid mount format: missing target");
    assert!(json.get("containerId").is_none());
    assert!(json.get("disallowedFeatureId").is_none());
    assert!(json.get("didStopContainer").is_none());
    assert!(json.get("learnMoreUrl").is_none());
}

#[test]
fn test_up_result_error_with_container_id() {
    let result = UpResult::error(
        "Lifecycle command failed".to_string(),
        "postCreateCommand exited with code 1".to_string(),
    )
    .with_container_id("container123".to_string());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["containerId"], "container123");
}

#[test]
fn test_up_result_error_with_disallowed_feature() {
    let result = UpResult::error(
        "Feature not allowed".to_string(),
        "Feature 'experimental-gpu' is not allowed in this environment".to_string(),
    )
    .with_disallowed_feature_id("experimental-gpu".to_string());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["disallowedFeatureId"], "experimental-gpu");
}

#[test]
fn test_up_result_error_with_did_stop_container() {
    let result = UpResult::error(
        "Container cleanup".to_string(),
        "Container was stopped during error handling".to_string(),
    )
    .with_did_stop_container(true);

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["didStopContainer"], true);
}

#[test]
fn test_up_result_error_with_learn_more_url() {
    let result = UpResult::error(
        "Configuration error".to_string(),
        "See documentation for more details".to_string(),
    )
    .with_learn_more_url("https://containers.dev/errors/config".to_string());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["learnMoreUrl"], "https://containers.dev/errors/config");
}

#[test]
fn test_up_result_json_roundtrip_success() {
    let original = UpResult::success(
        "container123".to_string(),
        "vscode".to_string(),
        "/workspaces/myproject".to_string(),
    );

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: UpResult = serde_json::from_str(&json).unwrap();

    assert_eq!(original, deserialized);
}

#[test]
fn test_up_result_json_roundtrip_error() {
    let original = UpResult::error("Test error".to_string(), "Test description".to_string())
        .with_container_id("container123".to_string())
        .with_disallowed_feature_id("test-feature".to_string());

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: UpResult = serde_json::from_str(&json).unwrap();

    assert_eq!(original, deserialized);
}
