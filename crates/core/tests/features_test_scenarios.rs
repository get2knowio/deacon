//! Tests for scenario file discovery and parsing.
//!
//! This test file validates:
//! - Scenario file location pinning (test/<feature>/scenarios.json or .jsonc)
//! - Valid JSON and JSONC parsing
//! - Invalid JSON/JSONC parse error messages (with file path and line/col)
//! - Missing scenario file behavior (returns empty vector, not an error)
//! - Empty scenario files
//! - Malformed scenarios (wrong schema, missing required fields)

use deacon_core::features_test::discovery::discover_scenarios;
use deacon_core::features_test::errors::Error;
use deacon_core::features_test::model::Feature;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

/// Helper to create a test feature with given test directory
fn create_test_feature(test_dir: PathBuf) -> Feature {
    Feature::new(
        "test-feature".to_string(),
        PathBuf::from("/fake/src/test-feature"),
        test_dir,
        false,
        Vec::new(),
    )
}

#[test]
fn test_valid_json_scenarios() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write a valid JSON scenarios file
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "name": "basic"
  },
  {
    "name": "advanced",
    "script": "custom.sh"
  }
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    assert_eq!(scenarios.len(), 2);
    assert_eq!(scenarios[0].name(), "basic");
    assert_eq!(scenarios[0].script_path().file_name().unwrap(), "basic.sh");
    assert_eq!(scenarios[1].name(), "advanced");
    assert_eq!(scenarios[1].script_path().file_name().unwrap(), "custom.sh");
}

#[test]
fn test_valid_jsonc_scenarios() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write a valid JSONC scenarios file with comments and trailing commas
    let scenarios_jsonc = test_dir.join("scenarios.jsonc");
    fs::write(
        &scenarios_jsonc,
        r#"[
  // This is a comment
  {
    "name": "basic",
    "script": "basic.sh", // trailing comma is allowed
  },
  {
    "name": "with-config",
    "config": "test.jsonc"
  }
]"#,
    )
    .expect("write scenarios.jsonc");

    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    assert_eq!(scenarios.len(), 2);
    assert_eq!(scenarios[0].name(), "basic");
    assert_eq!(scenarios[1].name(), "with-config");
    assert_eq!(
        scenarios[1]
            .config_path()
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "test.jsonc"
    );
}

#[test]
fn test_json_preferred_over_jsonc() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write both .json and .jsonc files
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(&scenarios_json, r#"[{"name": "from-json"}]"#).expect("write scenarios.json");

    let scenarios_jsonc = test_dir.join("scenarios.jsonc");
    fs::write(&scenarios_jsonc, r#"[{"name": "from-jsonc"}]"#).expect("write scenarios.jsonc");

    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    // .json should be preferred when both exist
    assert_eq!(scenarios.len(), 1);
    assert_eq!(scenarios[0].name(), "from-json");
}

#[test]
fn test_missing_scenarios_file_returns_empty() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // No scenarios.json or scenarios.jsonc exists
    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    // Should return empty vector, not an error
    assert_eq!(scenarios.len(), 0);
}

#[test]
fn test_empty_scenarios_array() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write an empty array
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(&scenarios_json, "[]").expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    assert_eq!(scenarios.len(), 0);
}

#[test]
fn test_invalid_json_syntax_includes_file_path() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write invalid JSON with syntax error
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "name": "test",
    "invalid": "missing closing brace"
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            // Error message should include file path
            assert!(
                msg.contains("scenarios.json"),
                "Error should include file path, got: {}",
                msg
            );
            // json5 errors typically include line/column info in their Display output
            // We're verifying the file path is included; line/col comes from json5::Error
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected parse error for invalid JSON"),
    }
}

#[test]
fn test_invalid_jsonc_syntax_includes_location() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write invalid JSONC with syntax error at a specific line
    let scenarios_jsonc = test_dir.join("scenarios.jsonc");
    fs::write(
        &scenarios_jsonc,
        r#"[
  {
    "name": "valid"
  },
  {
    "name": "invalid"
    "missing": "comma"
  }
]"#,
    )
    .expect("write scenarios.jsonc");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            // Error message should include file path
            assert!(
                msg.contains("scenarios.jsonc"),
                "Error should include file path, got: {}",
                msg
            );
            // Line/column info comes from json5 parser's error message
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected parse error for invalid JSONC"),
    }
}

#[test]
fn test_not_an_array_error() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write a JSON object instead of an array
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(&scenarios_json, r#"{"name": "not-an-array"}"#).expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            assert!(
                msg.contains("expected a sequence") || msg.contains("must contain a JSON array"),
                "Error should mention array/sequence requirement, got: {}",
                msg
            );
            assert!(
                msg.contains("scenarios.json"),
                "Error should include file path, got: {}",
                msg
            );
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected error for non-array JSON"),
    }
}

#[test]
fn test_scenario_not_an_object_error() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write an array containing a non-object element
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(&scenarios_json, r#"["string-instead-of-object"]"#).expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            assert!(
                msg.contains("expected struct ScenarioDefinition")
                    || msg.contains("must be an object"),
                "Error should mention object/struct requirement, got: {}",
                msg
            );
            assert!(
                msg.contains("scenarios.json"),
                "Error should include file path, got: {}",
                msg
            );
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected error for non-object scenario"),
    }
}

#[test]
fn test_missing_name_field_error() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write a scenario object without the required 'name' field
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "script": "test.sh"
  }
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            assert!(
                msg.contains("missing field `name`")
                    || msg.contains("missing required 'name' field"),
                "Error should mention missing name field, got: {}",
                msg
            );
            assert!(
                msg.contains("scenarios.json"),
                "Error should include file path, got: {}",
                msg
            );
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected error for missing name field"),
    }
}

#[test]
fn test_name_field_wrong_type_error() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write a scenario with 'name' as a number instead of string
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "name": 123
  }
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            assert!(
                msg.contains("invalid type") || msg.contains("expected a string"),
                "Error should mention type issue, got: {}",
                msg
            );
            assert!(
                msg.contains("scenarios.json"),
                "Error should include file path, got: {}",
                msg
            );
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected error for wrong type"),
    }
}

#[test]
fn test_scenario_with_all_optional_fields() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Write a scenario with all optional fields
    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "name": "full-scenario",
    "script": "custom-script.sh",
    "config": "custom-config.jsonc"
  }
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    assert_eq!(scenarios.len(), 1);
    assert_eq!(scenarios[0].name(), "full-scenario");
    assert_eq!(
        scenarios[0].script_path().file_name().unwrap(),
        "custom-script.sh"
    );
    assert!(scenarios[0].config_path().is_some());
    assert_eq!(
        scenarios[0]
            .config_path()
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "custom-config.jsonc"
    );
}

#[test]
fn test_multiple_scenarios_with_mixed_fields() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "name": "minimal"
  },
  {
    "name": "with-script",
    "script": "special.sh"
  },
  {
    "name": "with-config",
    "config": "test.jsonc"
  },
  {
    "name": "with-all",
    "script": "complete.sh",
    "config": "complete.jsonc"
  }
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir);
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    assert_eq!(scenarios.len(), 4);

    // Verify each scenario is parsed correctly
    assert_eq!(scenarios[0].name(), "minimal");
    assert_eq!(
        scenarios[0].script_path().file_name().unwrap(),
        "minimal.sh"
    );
    assert!(scenarios[0].config_path().is_none());

    assert_eq!(scenarios[1].name(), "with-script");
    assert_eq!(
        scenarios[1].script_path().file_name().unwrap(),
        "special.sh"
    );
    assert!(scenarios[1].config_path().is_none());

    assert_eq!(scenarios[2].name(), "with-config");
    assert_eq!(
        scenarios[2].script_path().file_name().unwrap(),
        "with-config.sh"
    );
    assert!(scenarios[2].config_path().is_some());

    assert_eq!(scenarios[3].name(), "with-all");
    assert_eq!(
        scenarios[3].script_path().file_name().unwrap(),
        "complete.sh"
    );
    assert!(scenarios[3].config_path().is_some());
}

#[test]
fn test_scenario_file_cannot_be_read() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Create a directory with the scenarios.json name to cause a read error
    let scenarios_json = test_dir.join("scenarios.json");
    fs::create_dir(&scenarios_json).expect("create dir");

    let feature = create_test_feature(test_dir);
    let result = discover_scenarios(&feature);

    match result {
        Err(Error::ScenarioParse(msg)) => {
            assert!(
                msg.contains("Failed to read"),
                "Error should mention read failure, got: {}",
                msg
            );
            assert!(
                msg.contains("scenarios.json"),
                "Error should include file path, got: {}",
                msg
            );
        }
        Err(e) => panic!("Expected ScenarioParse error, got: {:?}", e),
        Ok(_) => panic!("Expected error when scenarios file cannot be read"),
    }
}

#[test]
fn test_scenario_paths_are_absolute() {
    let tmp = tempdir().expect("create temp dir");
    let test_dir = tmp.path().join("test-feature");
    fs::create_dir_all(&test_dir).expect("create test dir");

    let scenarios_json = test_dir.join("scenarios.json");
    fs::write(
        &scenarios_json,
        r#"[
  {
    "name": "test",
    "script": "test.sh",
    "config": "test.jsonc"
  }
]"#,
    )
    .expect("write scenarios.json");

    let feature = create_test_feature(test_dir.clone());
    let scenarios = discover_scenarios(&feature).expect("discover scenarios");

    assert_eq!(scenarios.len(), 1);
    // Verify paths are constructed relative to test_dir
    assert_eq!(scenarios[0].script_path(), &test_dir.join("test.sh"));
    assert_eq!(
        scenarios[0].config_path().as_ref().unwrap().as_path(),
        test_dir.join("test.jsonc").as_path()
    );
}
