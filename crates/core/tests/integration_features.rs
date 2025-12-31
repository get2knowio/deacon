//! Integration tests for feature metadata parsing

use deacon_core::features::{parse_feature_metadata, FeatureOption, OptionValue};
use std::path::Path;

#[test]
fn test_parse_minimal_fixture() {
    let fixture_path = Path::new("../../fixtures/features/minimal/devcontainer-feature.json");
    let metadata = parse_feature_metadata(fixture_path).unwrap();

    assert_eq!(metadata.id, "minimal-feature");
    assert_eq!(metadata.name, None);
    assert_eq!(metadata.options.len(), 0);
    assert!(!metadata.has_lifecycle_commands());
}

#[test]
fn test_parse_feature_with_options_fixture() {
    let fixture_path = Path::new("../../fixtures/features/with-options/devcontainer-feature.json");
    let metadata = parse_feature_metadata(fixture_path).unwrap();

    assert_eq!(metadata.id, "feature-with-options");
    assert_eq!(metadata.version, Some("1.0.0".to_string()));
    assert_eq!(metadata.name, Some("Feature with Options".to_string()));
    assert_eq!(
        metadata.description,
        Some("A test feature with various option types".to_string())
    );

    // Test options
    assert_eq!(metadata.options.len(), 4);

    // Test boolean option
    let enable_option = metadata.options.get("enableFeature").unwrap();
    if let FeatureOption::Boolean { default, .. } = enable_option {
        assert_eq!(*default, Some(true));
    } else {
        panic!("Expected boolean option");
    }

    // Test string option with enum
    let version_option = metadata.options.get("version").unwrap();
    if let FeatureOption::String {
        default, r#enum, ..
    } = version_option
    {
        assert_eq!(*default, Some("stable".to_string()));
        assert_eq!(r#enum.as_ref().unwrap(), &vec!["latest", "stable", "beta"]);
    } else {
        panic!("Expected string option");
    }

    // Test container environment
    assert_eq!(
        metadata.container_env.get("FEATURE_ENABLED"),
        Some(&"true".to_string())
    );
    assert_eq!(
        metadata.container_env.get("FEATURE_PATH"),
        Some(&"/usr/local/feature".to_string())
    );

    // Test other properties
    assert_eq!(metadata.init, Some(true));
    assert_eq!(metadata.privileged, Some(false));
    assert_eq!(metadata.cap_add, vec!["SYS_PTRACE"]);
    assert_eq!(metadata.security_opt, vec!["seccomp=unconfined"]);
    assert_eq!(metadata.installs_after, vec!["common-utils"]);

    // Test lifecycle commands
    assert!(metadata.has_lifecycle_commands());
    assert!(metadata.on_create_command.is_some());
    assert!(metadata.post_create_command.is_some());
    assert!(metadata.post_start_command.is_some());
}

#[test]
fn test_parse_invalid_fixture() {
    let fixture_path =
        Path::new("../../fixtures/features/invalid-schema/devcontainer-feature.json");
    let result = parse_feature_metadata(fixture_path);

    assert!(result.is_ok()); // Parsing should succeed

    let metadata = result.unwrap();
    let validation_result = metadata.validate();
    assert!(validation_result.is_err());
    // Should fail validation because id is empty and default value doesn't match enum
}

#[test]
fn test_option_value_validation_with_fixtures() {
    let fixture_path = Path::new("../../fixtures/features/with-options/devcontainer-feature.json");
    let metadata = parse_feature_metadata(fixture_path).unwrap();

    // Test validation of version option (string with enum)
    let version_option = metadata.options.get("version").unwrap();

    // Valid value should pass
    assert!(version_option
        .validate_value(&OptionValue::String("stable".to_string()))
        .is_ok());
    assert!(version_option
        .validate_value(&OptionValue::String("latest".to_string()))
        .is_ok());

    // Invalid value should fail
    assert!(version_option
        .validate_value(&OptionValue::String("invalid".to_string()))
        .is_err());

    // Type mismatch should fail
    assert!(version_option
        .validate_value(&OptionValue::Boolean(true))
        .is_err());
}
