//! Integration tests for template metadata parsing and application

use deacon_core::features::FeatureOption;
use deacon_core::templates::{
    apply_template, parse_template_metadata, ApplyOptions, PlannedAction,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_parse_minimal_template_fixture() {
    let fixture_path = Path::new("../../fixtures/templates/minimal/devcontainer-template.json");
    let metadata = parse_template_metadata(fixture_path).unwrap();

    assert_eq!(metadata.id, "minimal-template");
    assert_eq!(metadata.name, Some("Minimal Template".to_string()));
    assert_eq!(
        metadata.description,
        Some("A minimal DevContainer template for testing".to_string())
    );
    assert_eq!(metadata.options.len(), 0);
    assert_eq!(metadata.files, None);
    assert_eq!(metadata.recommended_features, None);
}

#[test]
fn test_parse_template_with_options_fixture() {
    let fixture_path =
        Path::new("../../fixtures/templates/with-options/devcontainer-template.json");
    let metadata = parse_template_metadata(fixture_path).unwrap();

    assert_eq!(metadata.id, "template-with-options");
    assert_eq!(metadata.version, Some("1.0.0".to_string()));
    assert_eq!(metadata.name, Some("Template with Options".to_string()));
    assert_eq!(
        metadata.description,
        Some("A DevContainer template with various option types".to_string())
    );
    assert_eq!(
        metadata.documentation_url,
        Some("https://example.com/docs".to_string())
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

    // Test string option without enum
    let name_option = metadata.options.get("customName").unwrap();
    if let FeatureOption::String {
        default, r#enum, ..
    } = name_option
    {
        assert_eq!(*default, Some("my-project".to_string()));
        assert_eq!(*r#enum, None);
    } else {
        panic!("Expected string option");
    }

    // Test boolean option with false default
    let debug_option = metadata.options.get("debugMode").unwrap();
    if let FeatureOption::Boolean { default, .. } = debug_option {
        assert_eq!(*default, Some(false));
    } else {
        panic!("Expected boolean option");
    }

    // Test recommended features
    assert!(metadata.recommended_features.is_some());
    let features = metadata.recommended_features.as_ref().unwrap();
    assert!(features.is_object());

    // Test file list
    let files = metadata.files.as_ref().unwrap();
    assert_eq!(files.len(), 4);
    assert!(files.contains(&"Dockerfile".to_string()));
    assert!(files.contains(&"README.md".to_string()));
    assert!(files.contains(&"src/main.py".to_string()));
    assert!(files.contains(&"config/app.conf".to_string()));

    // Test other metadata
    assert_eq!(
        metadata.platforms,
        Some(vec!["linux".to_string(), "darwin".to_string()])
    );
    assert_eq!(metadata.publisher, Some("Test Publisher".to_string()));
    assert_eq!(
        metadata.keywords,
        Some(vec![
            "test".to_string(),
            "template".to_string(),
            "options".to_string()
        ])
    );
}

#[test]
fn test_apply_minimal_template_fixture() -> anyhow::Result<()> {
    let src_dir = Path::new("../../fixtures/templates/minimal");
    let temp_dir = TempDir::new()?;
    let dest_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&dest_dir)?;

    // Apply template
    let options = ApplyOptions::default();
    let result = apply_template(src_dir, &dest_dir, &options)?;

    // Check results - should copy Dockerfile and README.md, but not devcontainer-template.json
    assert_eq!(result.files_processed, 2);
    assert_eq!(result.files_skipped, 0);
    assert!(result.substitution_report.has_substitutions());

    // Verify files were created
    assert!(dest_dir.join("Dockerfile").exists());
    assert!(dest_dir.join("README.md").exists());
    assert!(!dest_dir.join("devcontainer-template.json").exists());

    // Check variable substitution in Dockerfile
    let dockerfile = fs::read_to_string(dest_dir.join("Dockerfile"))?;
    assert!(dockerfile.contains(&dest_dir.to_string_lossy().to_string()));
    assert!(!dockerfile.contains("${localWorkspaceFolder}"));

    // Check variable substitution in README.md
    let readme = fs::read_to_string(dest_dir.join("README.md"))?;
    assert!(readme.contains(&dest_dir.to_string_lossy().to_string()));
    assert!(!readme.contains("${localWorkspaceFolder}"));

    Ok(())
}

#[test]
fn test_apply_template_with_options_fixture() -> anyhow::Result<()> {
    let src_dir = Path::new("../../fixtures/templates/with-options");
    let temp_dir = TempDir::new()?;
    let dest_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&dest_dir)?;

    // Apply template
    let options = ApplyOptions::default();
    let result = apply_template(src_dir, &dest_dir, &options)?;

    // Check results - should copy all files except devcontainer-template.json
    assert_eq!(result.files_processed, 4);
    assert_eq!(result.files_skipped, 0);
    assert!(result.substitution_report.has_substitutions());

    // Verify files and directories were created
    assert!(dest_dir.join("Dockerfile").exists());
    assert!(dest_dir.join("README.md").exists());
    assert!(dest_dir.join("src").is_dir());
    assert!(dest_dir.join("src/main.py").exists());
    assert!(dest_dir.join("config").is_dir());
    assert!(dest_dir.join("config/app.conf").exists());
    assert!(!dest_dir.join("devcontainer-template.json").exists());

    // Check variable substitution in various files
    let dockerfile = fs::read_to_string(dest_dir.join("Dockerfile"))?;
    assert!(dockerfile.contains(&dest_dir.to_string_lossy().to_string()));
    assert!(!dockerfile.contains("${localWorkspaceFolder}"));

    let main_py = fs::read_to_string(dest_dir.join("src/main.py"))?;
    assert!(main_py.contains(&dest_dir.to_string_lossy().to_string()));
    assert!(!main_py.contains("${localWorkspaceFolder}"));

    let app_conf = fs::read_to_string(dest_dir.join("config/app.conf"))?;
    assert!(app_conf.contains(&dest_dir.to_string_lossy().to_string()));
    assert!(!app_conf.contains("${localWorkspaceFolder}"));

    let readme = fs::read_to_string(dest_dir.join("README.md"))?;
    assert!(readme.contains(&dest_dir.to_string_lossy().to_string()));
    assert!(!readme.contains("${localWorkspaceFolder}"));

    Ok(())
}

#[test]
fn test_apply_template_dry_run_fixture() -> anyhow::Result<()> {
    let src_dir = Path::new("../../fixtures/templates/with-options");
    let temp_dir = TempDir::new()?;
    let dest_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&dest_dir)?;

    // Apply template in dry run mode
    let options = ApplyOptions {
        dry_run: true,
        ..Default::default()
    };

    let result = apply_template(src_dir, &dest_dir, &options)?;

    // Check results
    assert_eq!(result.files_processed, 4);
    assert_eq!(result.files_skipped, 0);
    assert_eq!(result.actions.len(), 4);

    // Verify no files were actually created
    assert!(!dest_dir.join("Dockerfile").exists());
    assert!(!dest_dir.join("README.md").exists());
    assert!(!dest_dir.join("src/main.py").exists());
    assert!(!dest_dir.join("config/app.conf").exists());

    // Check planned actions
    let mut dockerfile_action_found = false;
    let mut main_py_action_found = false;

    for action in &result.actions {
        match action {
            PlannedAction::CopyFile {
                src,
                dest,
                has_substitutions,
            } => {
                if src.file_name().unwrap() == "Dockerfile" {
                    dockerfile_action_found = true;
                    assert!(*has_substitutions);
                    assert_eq!(dest.file_name().unwrap(), "Dockerfile");
                }
                if src.file_name().unwrap() == "main.py" {
                    main_py_action_found = true;
                    assert!(*has_substitutions);
                    assert_eq!(dest.file_name().unwrap(), "main.py");
                }
            }
            _ => panic!("Expected CopyFile action in dry run"),
        }
    }

    assert!(dockerfile_action_found);
    assert!(main_py_action_found);

    Ok(())
}
