//! Unit tests for the up command.

use super::args::{normalize_and_validate_args, MountType, NormalizedMount, UpArgs};
use super::result::UpResult;
use crate::commands::shared::NormalizedRemoteEnv;
use deacon_core::config::DevContainerConfig;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn test_up_args_creation() {
    let args = UpArgs {
        remove_existing_container: true,
        workspace_folder: Some(PathBuf::from("/test")),
        ..Default::default()
    };

    assert!(args.remove_existing_container);
    assert!(!args.skip_post_create);
    assert!(!args.skip_non_blocking_commands);
    assert!(!args.ports_events);
    assert!(!args.shutdown);
    assert_eq!(args.workspace_folder, Some(PathBuf::from("/test")));
    assert!(args.config_path.is_none());
}

#[test]
fn test_error_mapping_config_not_found() {
    use deacon_core::errors::{ConfigError, DeaconError};

    let error = DeaconError::Config(ConfigError::NotFound {
        path: "/workspace/devcontainer.json".to_string(),
    });
    let result = UpResult::from_error(error.into());

    assert!(result.is_error());
    if let UpResult::Error(err) = result {
        assert_eq!(err.outcome, "error");
        assert_eq!(err.message, "No devcontainer.json found in workspace");
        assert!(err.description.contains("/workspace/devcontainer.json"));
    } else {
        panic!("Expected Error variant");
    }
}

#[test]
fn test_error_mapping_validation_error() {
    use deacon_core::errors::{ConfigError, DeaconError};

    let error = DeaconError::Config(ConfigError::Validation {
        message: "Invalid mount format: missing target".to_string(),
    });
    let result = UpResult::from_error(error.into());

    assert!(result.is_error());
    if let UpResult::Error(err) = result {
        assert_eq!(err.outcome, "error");
        assert_eq!(err.message, "Invalid configuration or arguments");
        assert_eq!(err.description, "Invalid mount format: missing target");
    } else {
        panic!("Expected Error variant");
    }
}

#[test]
fn test_error_mapping_docker_error() {
    use deacon_core::errors::{DeaconError, DockerError};

    let error = DeaconError::Docker(DockerError::ContainerNotFound {
        id: "abc123".to_string(),
    });
    let result = UpResult::from_error(error.into());

    assert!(result.is_error());
    if let UpResult::Error(err) = result {
        assert_eq!(err.outcome, "error");
        assert_eq!(err.message, "Container not found");
        assert!(err.description.contains("abc123"));
    } else {
        panic!("Expected Error variant");
    }
}

#[test]
fn test_error_mapping_network_error() {
    use deacon_core::errors::DeaconError;

    let error = DeaconError::Network {
        message: "Connection timeout".to_string(),
    };
    let result = UpResult::from_error(error.into());

    assert!(result.is_error());
    if let UpResult::Error(err) = result {
        assert_eq!(err.outcome, "error");
        assert_eq!(err.message, "Network error");
        assert_eq!(err.description, "Connection timeout");
    } else {
        panic!("Expected Error variant");
    }
}

#[test]
fn test_up_args_with_all_flags() {
    let args = UpArgs {
        remove_existing_container: true,
        skip_post_create: true,
        skip_non_blocking_commands: true,
        ports_events: true,
        shutdown: true,
        forward_ports: vec!["8080".to_string(), "3000:3000".to_string()],
        workspace_folder: Some(PathBuf::from("/test")),
        ..Default::default()
    };

    assert!(args.remove_existing_container);
    assert!(args.skip_post_create);
    assert!(args.skip_non_blocking_commands);
    assert!(args.ports_events);
    assert!(args.shutdown);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_compose_config_detection() {
    let mut compose_config = DevContainerConfig::default();
    compose_config.name = Some("Test Compose".to_string());
    compose_config.docker_compose_file = Some(json!("docker-compose.yml"));
    compose_config.service = Some("app".to_string());
    compose_config.run_services = vec!["db".to_string()];
    compose_config.shutdown_action = Some("stopCompose".to_string());
    compose_config.post_create_command = Some(json!("echo 'Container ready'"));

    assert!(compose_config.uses_compose());
    assert!(compose_config.has_stop_compose_shutdown());
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_traditional_config_detection() {
    let mut traditional_config = DevContainerConfig::default();
    traditional_config.name = Some("Test Traditional".to_string());
    traditional_config.image = Some("node:18".to_string());

    assert!(!traditional_config.uses_compose());
    assert!(!traditional_config.has_stop_compose_shutdown());
}

#[test]
fn test_cli_forward_ports_merging() {
    use deacon_core::config::PortSpec;

    // Start with a config that has some ports
    let mut config = DevContainerConfig {
        forward_ports: vec![PortSpec::Number(3000), PortSpec::Number(4000)],
        ..Default::default()
    };

    // Simulate CLI forward ports
    let cli_ports = vec!["8080".to_string(), "5000:5000".to_string()];

    // Merge CLI ports into config using shared parser
    for port_str in &cli_ports {
        if let Ok(port_spec) = PortSpec::parse(port_str) {
            config.forward_ports.push(port_spec);
        }
    }

    // Verify merged ports
    assert_eq!(config.forward_ports.len(), 4);
    assert!(matches!(config.forward_ports[0], PortSpec::Number(3000)));
    assert!(matches!(config.forward_ports[1], PortSpec::Number(4000)));
    assert!(matches!(config.forward_ports[2], PortSpec::Number(8080)));
    assert!(matches!(
        config.forward_ports[3],
        PortSpec::String(ref s) if s == "5000:5000"
    ));
}

#[test]
fn test_forward_ports_parsing() {
    use deacon_core::config::PortSpec;

    // Test single port number
    let port_spec = PortSpec::parse("8080").unwrap();
    assert!(matches!(port_spec, PortSpec::Number(8080)));

    // Test port mapping
    let port_spec = PortSpec::parse("3000:3000").unwrap();
    assert!(matches!(
        port_spec,
        PortSpec::String(ref s) if s == "3000:3000"
    ));

    // Test host:container mapping
    let port_spec = PortSpec::parse("8080:3000").unwrap();
    assert!(matches!(
        port_spec,
        PortSpec::String(ref s) if s == "8080:3000"
    ));

    // Test invalid port
    assert!(PortSpec::parse("invalid").is_err());

    // Test invalid port mapping
    assert!(PortSpec::parse("8080:invalid").is_err());
}

#[test]
fn test_normalized_mount_parse_bind() {
    let mount =
        NormalizedMount::parse("type=bind,source=/host/path,target=/container/path").unwrap();
    assert!(matches!(mount.mount_type, MountType::Bind));
    assert_eq!(mount.source, "/host/path");
    assert_eq!(mount.target, "/container/path");
    assert!(!mount.read_only);
}

#[test]
fn test_normalized_mount_parse_volume_with_external() {
    let mount =
        NormalizedMount::parse("type=volume,source=myvolume,target=/data,external=true").unwrap();
    assert!(matches!(mount.mount_type, MountType::Volume));
    assert_eq!(mount.source, "myvolume");
    assert_eq!(mount.target, "/data");
    assert!(mount.read_only);
    assert!(mount.consistency.is_none());
}

#[test]
fn test_normalized_mount_parse_with_consistency_cached() {
    let mount = NormalizedMount::parse(
        "type=bind,source=/host/path,target=/container/path,consistency=cached",
    )
    .unwrap();
    assert!(matches!(mount.mount_type, MountType::Bind));
    assert_eq!(mount.source, "/host/path");
    assert_eq!(mount.target, "/container/path");
    assert!(!mount.read_only);
    assert_eq!(mount.consistency, Some("cached".to_string()));
}

#[test]
fn test_normalized_mount_parse_with_consistency_delegated() {
    let mount = NormalizedMount::parse(
        "type=bind,source=/host/path,target=/container/path,consistency=delegated",
    )
    .unwrap();
    assert_eq!(mount.consistency, Some("delegated".to_string()));
}

#[test]
fn test_normalized_mount_parse_with_consistency_consistent() {
    let mount = NormalizedMount::parse(
        "type=bind,source=/host/path,target=/container/path,consistency=consistent",
    )
    .unwrap();
    assert_eq!(mount.consistency, Some("consistent".to_string()));
}

#[test]
fn test_normalized_mount_parse_with_external_and_consistency() {
    let mount = NormalizedMount::parse(
        "type=bind,source=/host/path,target=/container/path,external=true,consistency=cached",
    )
    .unwrap();
    assert!(mount.read_only);
    assert_eq!(mount.consistency, Some("cached".to_string()));
}

#[test]
fn test_normalized_mount_parse_with_consistency_before_external() {
    // Test that options can appear in any order
    let mount = NormalizedMount::parse(
        "type=bind,source=/host/path,target=/container/path,consistency=cached,external=true",
    )
    .unwrap();
    assert!(mount.read_only);
    assert_eq!(mount.consistency, Some("cached".to_string()));
}

#[test]
fn test_normalized_mount_to_spec_string_with_consistency() {
    let mount = NormalizedMount {
        mount_type: MountType::Bind,
        source: "/host/path".to_string(),
        target: "/container/path".to_string(),
        read_only: false,
        consistency: Some("cached".to_string()),
    };
    let spec_string = mount.to_spec_string();
    assert!(spec_string.contains("consistency=cached"));
    assert_eq!(
        spec_string,
        "type=bind,source=/host/path,target=/container/path,consistency=cached"
    );
}

#[test]
fn test_normalized_mount_to_spec_string_with_external_and_consistency() {
    let mount = NormalizedMount {
        mount_type: MountType::Bind,
        source: "/host/path".to_string(),
        target: "/container/path".to_string(),
        read_only: true,
        consistency: Some("delegated".to_string()),
    };
    let spec_string = mount.to_spec_string();
    assert!(spec_string.contains("external=true"));
    assert!(spec_string.contains("consistency=delegated"));
}

#[test]
fn test_normalized_mount_parse_invalid_format() {
    // Missing target
    assert!(NormalizedMount::parse("type=bind,source=/tmp").is_err());

    // Invalid type
    assert!(NormalizedMount::parse("type=invalid,source=/tmp,target=/data").is_err());

    // Missing source
    assert!(NormalizedMount::parse("type=bind,target=/data").is_err());
}

#[test]
fn test_normalized_remote_env_parse_valid() {
    let env = NormalizedRemoteEnv::parse("FOO=bar").unwrap();
    assert_eq!(env.name, "FOO");
    assert_eq!(env.value, "bar");

    // Test with equals in value
    let env = NormalizedRemoteEnv::parse("DATABASE_URL=postgres://user:pass@host/db").unwrap();
    assert_eq!(env.name, "DATABASE_URL");
    assert_eq!(env.value, "postgres://user:pass@host/db");
}

#[test]
fn test_normalized_remote_env_parse_invalid() {
    // Missing equals sign
    assert!(NormalizedRemoteEnv::parse("INVALID").is_err());

    // Empty value is ok (equals present)
    let env = NormalizedRemoteEnv::parse("EMPTY=").unwrap();
    assert_eq!(env.name, "EMPTY");
    assert_eq!(env.value, "");
}

#[test]
fn test_invalid_id_label_uses_shared_selector_message() {
    let args = UpArgs {
        id_label: vec!["foo".to_string()],
        workspace_folder: Some(PathBuf::from("/tmp/workspace")),
        ..Default::default()
    };

    let err = normalize_and_validate_args(&args).unwrap_err();
    let result = UpResult::from_error(err);

    if let UpResult::Error(err_payload) = result {
        assert_eq!(
            err_payload.description,
            "Unmatched argument format: id-label must match <name>=<value>."
        );
        assert_eq!(err_payload.message, "Invalid configuration or arguments");
    } else {
        panic!("Expected error result for invalid id-label input");
    }
}

#[tokio::test]
async fn test_canonicalization_failure() {
    use super::execute_up;

    // Test that proper errors are returned when workspace path cannot be canonicalized
    // This tests the canonicalization logic in execute_up function
    let non_existent_path = PathBuf::from("/nonexistent/path/that/does/not/exist/xyz123");
    let args = UpArgs {
        workspace_folder: Some(non_existent_path.clone()),
        mount_workspace_git_root: false, // This will trigger canonicalize path
        ..Default::default()
    };

    // Call execute_up which will attempt canonicalization
    let result = execute_up(args).await;

    // Verify the error is properly returned
    assert!(
        result.is_err(),
        "Expected canonicalization to fail for non-existent path"
    );

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("Failed to resolve workspace path")
            || err_str.contains("No such file or directory")
            || err_str.contains("cannot be accessed"),
        "Error should mention path resolution failure, got: {}",
        err_str
    );
}

#[test]
fn test_effective_mount_serialization() {
    use super::result::EffectiveMount;

    // Test serialization with options
    let mount = EffectiveMount {
        source: "/host/path".to_string(),
        target: "/container/path".to_string(),
        options: vec!["ro".to_string(), "consistency=cached".to_string()],
    };

    let json_value = serde_json::to_value(&mount).unwrap();
    assert_eq!(json_value["source"], "/host/path");
    assert_eq!(json_value["target"], "/container/path");
    assert_eq!(json_value["options"], json!(["ro", "consistency=cached"]));

    // Test serialization with empty options (should skip the field)
    let mount_no_options = EffectiveMount {
        source: "/host/path".to_string(),
        target: "/container/path".to_string(),
        options: vec![],
    };

    let json_value_no_options = serde_json::to_value(&mount_no_options).unwrap();
    assert!(json_value_no_options.get("options").is_none());
}

#[test]
fn test_up_success_with_new_fields_serialization() {
    use super::result::{EffectiveMount, UpSuccess};
    use std::collections::HashMap;

    let mut effective_env = HashMap::new();
    effective_env.insert("GITHUB_TOKEN".to_string(), "***".to_string());
    effective_env.insert("NODE_ENV".to_string(), "development".to_string());

    let success = UpSuccess {
        outcome: "success".to_string(),
        container_id: "abc123".to_string(),
        compose_project_name: Some("myproject".to_string()),
        remote_user: "vscode".to_string(),
        remote_workspace_folder: "/workspaces/myproject".to_string(),
        effective_mounts: Some(vec![
            EffectiveMount {
                source: "/home/user/code".to_string(),
                target: "/workspaces/myproject".to_string(),
                options: vec![],
            },
            EffectiveMount {
                source: "/home/user/.gitconfig".to_string(),
                target: "/home/vscode/.gitconfig".to_string(),
                options: vec!["ro".to_string()],
            },
        ]),
        effective_env: Some(effective_env),
        profiles_applied: Some(vec!["dev".to_string(), "debug".to_string()]),
        external_volumes_preserved: Some(vec![
            "postgres_data".to_string(),
            "redis_data".to_string(),
        ]),
        configuration: None,
        merged_configuration: None,
    };

    let json_value = serde_json::to_value(&success).unwrap();

    // Verify all fields are correctly serialized
    assert_eq!(json_value["outcome"], "success");
    assert_eq!(json_value["containerId"], "abc123");
    assert_eq!(json_value["composeProjectName"], "myproject");
    assert_eq!(json_value["remoteUser"], "vscode");
    assert_eq!(json_value["remoteWorkspaceFolder"], "/workspaces/myproject");

    // Check effective mounts
    let mounts = json_value["effectiveMounts"].as_array().unwrap();
    assert_eq!(mounts.len(), 2);
    assert_eq!(mounts[0]["source"], "/home/user/code");
    assert_eq!(mounts[0]["target"], "/workspaces/myproject");
    assert!(mounts[0].get("options").is_none()); // Empty options should be omitted
    assert_eq!(mounts[1]["source"], "/home/user/.gitconfig");
    assert_eq!(mounts[1]["options"], json!(["ro"]));

    // Check effective env
    let env = json_value["effectiveEnv"].as_object().unwrap();
    assert!(env.contains_key("GITHUB_TOKEN"));
    assert!(env.contains_key("NODE_ENV"));

    // Check profiles
    let profiles = json_value["profilesApplied"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    assert!(profiles.contains(&json!("dev")));
    assert!(profiles.contains(&json!("debug")));

    // Check external volumes
    let volumes = json_value["externalVolumesPreserved"].as_array().unwrap();
    assert_eq!(volumes.len(), 2);
    assert!(volumes.contains(&json!("postgres_data")));
    assert!(volumes.contains(&json!("redis_data")));
}

#[test]
fn test_up_success_with_none_fields_serialization() {
    use super::result::UpSuccess;

    // Test that None fields are correctly omitted
    let success = UpSuccess {
        outcome: "success".to_string(),
        container_id: "abc123".to_string(),
        compose_project_name: None,
        remote_user: "root".to_string(),
        remote_workspace_folder: "/workspaces".to_string(),
        effective_mounts: None,
        effective_env: None,
        profiles_applied: None,
        external_volumes_preserved: None,
        configuration: None,
        merged_configuration: None,
    };

    let json_value = serde_json::to_value(&success).unwrap();
    let json_obj = json_value.as_object().unwrap();

    // These fields should be present
    assert!(json_obj.contains_key("outcome"));
    assert!(json_obj.contains_key("containerId"));
    assert!(json_obj.contains_key("remoteUser"));
    assert!(json_obj.contains_key("remoteWorkspaceFolder"));

    // These fields should be omitted when None
    assert!(!json_obj.contains_key("composeProjectName"));
    assert!(!json_obj.contains_key("effectiveMounts"));
    assert!(!json_obj.contains_key("effectiveEnv"));
    assert!(!json_obj.contains_key("profilesApplied"));
    assert!(!json_obj.contains_key("externalVolumesPreserved"));
    assert!(!json_obj.contains_key("configuration"));
    assert!(!json_obj.contains_key("mergedConfiguration"));
}

#[test]
fn test_up_result_builder_methods_for_new_fields() {
    use super::result::EffectiveMount;
    use std::collections::HashMap;

    let mut result = UpResult::success(
        "container123".to_string(),
        "user".to_string(),
        "/workspaces".to_string(),
    );

    // Add effective mounts
    let mounts = vec![EffectiveMount {
        source: "/host".to_string(),
        target: "/container".to_string(),
        options: vec!["ro".to_string()],
    }];
    result = result.with_effective_mounts(mounts);

    // Add effective env
    let mut env = HashMap::new();
    env.insert("TEST".to_string(), "value".to_string());
    result = result.with_effective_env(env);

    // Add profiles
    result = result.with_profiles_applied(vec!["dev".to_string()]);

    // Add external volumes
    result = result.with_external_volumes_preserved(vec!["data".to_string()]);

    // Verify the fields are set
    if let UpResult::Success(success) = &result {
        assert!(success.effective_mounts.is_some());
        assert_eq!(success.effective_mounts.as_ref().unwrap().len(), 1);

        assert!(success.effective_env.is_some());
        assert!(success.effective_env.as_ref().unwrap().contains_key("TEST"));

        assert!(success.profiles_applied.is_some());
        assert_eq!(
            success.profiles_applied.as_ref().unwrap(),
            &vec!["dev".to_string()]
        );

        assert!(success.external_volumes_preserved.is_some());
        assert_eq!(
            success.external_volumes_preserved.as_ref().unwrap(),
            &vec!["data".to_string()]
        );
    } else {
        panic!("Expected Success variant");
    }
}
