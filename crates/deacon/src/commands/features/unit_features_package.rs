use std::fs;
use tempfile::TempDir;

// Import the helper functions we're testing
use crate::commands::features::{
    create_feature_tgz, detect_mode, enumerate_and_validate_collection, validate_single,
    write_collection_metadata, CollectionMetadata, FeatureDescriptor, PackagingMode,
    SourceInformation,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test detect_mode function
    mod test_detect_mode {
        use super::*;

        #[test]
        fn detects_single_feature_with_devcontainer_feature_json() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            // Create devcontainer-feature.json
            let feature_json = feature_dir.join("devcontainer-feature.json");
            fs::write(&feature_json, r#"{"id": "my-feature", "version": "1.0.0"}"#).unwrap();

            let mode = detect_mode(&feature_dir).unwrap();
            assert_eq!(mode, PackagingMode::Single);
        }

        #[test]
        fn detects_collection_with_src_directory() {
            let temp_dir = TempDir::new().unwrap();
            let collection_dir = temp_dir.path().join("my-collection");
            fs::create_dir(&collection_dir).unwrap();

            // Create src/ directory
            let src_dir = collection_dir.join("src");
            fs::create_dir(&src_dir).unwrap();

            let mode = detect_mode(&collection_dir).unwrap();
            assert_eq!(mode, PackagingMode::Collection);
        }

        #[test]
        fn returns_error_for_invalid_directory() {
            let temp_dir = TempDir::new().unwrap();
            let invalid_dir = temp_dir.path().join("invalid");
            fs::create_dir(&invalid_dir).unwrap();

            let result = detect_mode(&invalid_dir);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Cannot determine packaging mode"));
        }

        #[test]
        fn returns_error_for_nonexistent_directory() {
            let temp_dir = TempDir::new().unwrap();
            let nonexistent = temp_dir.path().join("nonexistent");

            let result = detect_mode(&nonexistent);
            assert!(result.is_err());
        }
    }

    /// Test validate_single function
    mod test_validate_single {
        use super::*;

        #[test]
        fn validates_valid_single_feature() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            // Create valid devcontainer-feature.json
            let feature_json = feature_dir.join("devcontainer-feature.json");
            fs::write(
                &feature_json,
                r#"{
                "id": "my-feature",
                "version": "1.0.0",
                "name": "My Feature",
                "description": "A test feature"
            }"#,
            )
            .unwrap();

            let metadata = validate_single(&feature_dir).unwrap();
            assert_eq!(metadata.id, "my-feature");
            assert_eq!(metadata.version, Some("1.0.0".to_string()));
        }

        #[test]
        fn rejects_missing_devcontainer_feature_json() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            let result = validate_single(&feature_dir);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("devcontainer-feature.json not found"));
        }

        #[test]
        fn rejects_directory_as_devcontainer_feature_json() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            // Create a directory named devcontainer-feature.json
            let feature_json_dir = feature_dir.join("devcontainer-feature.json");
            fs::create_dir(&feature_json_dir).unwrap();

            let result = validate_single(&feature_dir);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("is not a file"));
        }

        #[test]
        fn rejects_missing_version_field() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            // Create devcontainer-feature.json without version
            let feature_json = feature_dir.join("devcontainer-feature.json");
            fs::write(
                &feature_json,
                r#"{
                "id": "my-feature",
                "name": "My Feature"
            }"#,
            )
            .unwrap();

            let result = validate_single(&feature_dir);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("missing required 'version' field"));
        }

        #[test]
        fn rejects_invalid_json() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            // Create invalid JSON
            let feature_json = feature_dir.join("devcontainer-feature.json");
            fs::write(&feature_json, r#"{"id": "my-feature", "version":}"#).unwrap();

            let result = validate_single(&feature_dir);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Failed to parse"));
        }
    }

    /// Test enumerate_and_validate_collection function
    mod test_enumerate_and_validate_collection {
        use super::*;

        #[test]
        fn enumerates_valid_collection() {
            let temp_dir = TempDir::new().unwrap();
            let collection_dir = temp_dir.path().join("my-collection");
            fs::create_dir(&collection_dir).unwrap();

            let src_dir = collection_dir.join("src");
            fs::create_dir(&src_dir).unwrap();

            // Create two valid features
            for feature_id in ["feature-a", "feature-b"] {
                let feature_dir = src_dir.join(feature_id);
                fs::create_dir(&feature_dir).unwrap();

                let feature_json = feature_dir.join("devcontainer-feature.json");
                fs::write(
                    &feature_json,
                    format!(
                        r#"{{
                    "id": "{}",
                    "version": "1.0.0",
                    "name": "{}"
                }}"#,
                        feature_id, feature_id
                    ),
                )
                .unwrap();
            }

            let features = enumerate_and_validate_collection(&src_dir).unwrap();

            // Should be sorted by feature ID
            assert_eq!(features.len(), 2);
            assert_eq!(features[0].0, "feature-a");
            assert_eq!(features[1].0, "feature-b");
            assert_eq!(features[0].2.id, "feature-a");
            assert_eq!(features[1].2.id, "feature-b");
        }

        #[test]
        fn rejects_missing_src_directory() {
            let temp_dir = TempDir::new().unwrap();
            let nonexistent_src = temp_dir.path().join("nonexistent");

            let result = enumerate_and_validate_collection(&nonexistent_src);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("src/ directory not found"));
        }

        #[test]
        fn rejects_src_as_file() {
            let temp_dir = TempDir::new().unwrap();
            let src_file = temp_dir.path().join("src");
            fs::write(&src_file, "not a directory").unwrap();

            let result = enumerate_and_validate_collection(&src_file);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("is not a directory"));
        }

        #[test]
        fn rejects_empty_collection() {
            let temp_dir = TempDir::new().unwrap();
            let src_dir = temp_dir.path().join("src");
            fs::create_dir(&src_dir).unwrap();

            let result = enumerate_and_validate_collection(&src_dir);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("No valid features found"));
        }

        #[test]
        fn rejects_invalid_feature_in_collection() {
            let temp_dir = TempDir::new().unwrap();
            let collection_dir = temp_dir.path().join("my-collection");
            fs::create_dir(&collection_dir).unwrap();

            let src_dir = collection_dir.join("src");
            fs::create_dir(&src_dir).unwrap();

            // Create one valid feature
            let valid_feature_dir = src_dir.join("valid-feature");
            fs::create_dir(&valid_feature_dir).unwrap();
            let valid_feature_json = valid_feature_dir.join("devcontainer-feature.json");
            fs::write(
                &valid_feature_json,
                r#"{
                "id": "valid-feature",
                "version": "1.0.0"
            }"#,
            )
            .unwrap();

            // Create one invalid feature (missing version)
            let invalid_feature_dir = src_dir.join("invalid-feature");
            fs::create_dir(&invalid_feature_dir).unwrap();
            let invalid_feature_json = invalid_feature_dir.join("devcontainer-feature.json");
            fs::write(
                &invalid_feature_json,
                r#"{
                "id": "invalid-feature"
            }"#,
            )
            .unwrap();

            let result = enumerate_and_validate_collection(&src_dir);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("invalid feature"));
        }

        #[test]
        fn rejects_feature_id_mismatch() {
            let temp_dir = TempDir::new().unwrap();
            let collection_dir = temp_dir.path().join("my-collection");
            fs::create_dir(&collection_dir).unwrap();

            let src_dir = collection_dir.join("src");
            fs::create_dir(&src_dir).unwrap();

            // Create feature with mismatched ID
            let feature_dir = src_dir.join("directory-name");
            fs::create_dir(&feature_dir).unwrap();
            let feature_json = feature_dir.join("devcontainer-feature.json");
            fs::write(
                &feature_json,
                r#"{
                "id": "different-id",
                "version": "1.0.0"
            }"#,
            )
            .unwrap();

            let result = enumerate_and_validate_collection(&src_dir);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("does not match directory name"));
        }
    }

    /// Test write_collection_metadata function
    mod test_write_collection_metadata {
        use super::*;

        #[test]
        fn writes_collection_metadata() {
            let temp_dir = TempDir::new().unwrap();
            let output_file = temp_dir.path().join("devcontainer-collection.json");

            let metadata = CollectionMetadata {
                source_information: SourceInformation {
                    source: "test".to_string(),
                },
                features: {
                    let mut map = std::collections::BTreeMap::new();
                    map.insert(
                        "feature-a".to_string(),
                        FeatureDescriptor {
                            id: "feature-a".to_string(),
                            version: "1.0.0".to_string(),
                            description: Some("Feature A".to_string()),
                            name: None,
                            options: None,
                            installs_after: None,
                            depends_on: None,
                        },
                    );
                    map.insert(
                        "feature-b".to_string(),
                        FeatureDescriptor {
                            id: "feature-b".to_string(),
                            version: "2.0.0".to_string(),
                            description: Some("Feature B".to_string()),
                            name: None,
                            options: None,
                            installs_after: None,
                            depends_on: None,
                        },
                    );
                    map
                },
            };

            write_collection_metadata(&metadata, &output_file).unwrap();

            // Verify file was created and contains expected content
            assert!(output_file.exists());
            let content = fs::read_to_string(&output_file).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

            assert_eq!(parsed["source_information"]["source"], "test");
            assert_eq!(parsed["features"].as_object().unwrap().len(), 2);
            assert_eq!(parsed["features"]["feature-a"]["id"], "feature-a");
            assert_eq!(parsed["features"]["feature-b"]["id"], "feature-b");
        }

        #[test]
        fn creates_parent_directories() {
            let temp_dir = TempDir::new().unwrap();
            let nested_file = temp_dir
                .path()
                .join("nested")
                .join("deep")
                .join("devcontainer-collection.json");

            let metadata = CollectionMetadata {
                source_information: SourceInformation {
                    source: "test".to_string(),
                },
                features: std::collections::BTreeMap::new(),
            };

            write_collection_metadata(&metadata, &nested_file).unwrap();
            assert!(nested_file.exists());
        }
    }

    /// Test create_feature_tgz function
    mod test_create_feature_tgz {
        use super::*;

        #[test]
        fn creates_tgz_archive_and_returns_digest() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();

            // Create some test files
            fs::write(
                feature_dir.join("devcontainer-feature.json"),
                r#"{
                "id": "my-feature",
                "version": "1.0.0"
            }"#,
            )
            .unwrap();

            fs::write(
                feature_dir.join("install.sh"),
                "#!/bin/bash\necho 'install'",
            )
            .unwrap();

            let subdir = feature_dir.join("subdir");
            fs::create_dir(&subdir).unwrap();
            fs::write(subdir.join("file.txt"), "content").unwrap();

            let archive_path = temp_dir.path().join("feature.tgz");

            let digest = create_feature_tgz(&feature_dir, &archive_path).unwrap();

            // Verify archive was created
            assert!(archive_path.exists());

            // Verify digest format
            assert!(digest.starts_with("sha256:"));

            // Verify we can extract the archive (basic smoke test)
            let extract_dir = temp_dir.path().join("extracted");
            fs::create_dir(&extract_dir).unwrap();

            let archive_file = fs::File::open(&archive_path).unwrap();
            let tar = flate2::read::GzDecoder::new(archive_file);
            let mut archive = tar::Archive::new(tar);
            archive.unpack(&extract_dir).unwrap();

            // Verify extracted files
            assert!(extract_dir.join("devcontainer-feature.json").exists());
            assert!(extract_dir.join("install.sh").exists());
            assert!(extract_dir.join("subdir").join("file.txt").exists());
        }

        #[test]
        fn creates_parent_directories_for_archive() {
            let temp_dir = TempDir::new().unwrap();
            let feature_dir = temp_dir.path().join("my-feature");
            fs::create_dir(&feature_dir).unwrap();
            fs::write(
                feature_dir.join("devcontainer-feature.json"),
                r#"{
                "id": "my-feature",
                "version": "1.0.0"
            }"#,
            )
            .unwrap();

            let nested_archive = temp_dir
                .path()
                .join("nested")
                .join("deep")
                .join("feature.tgz");

            let digest = create_feature_tgz(&feature_dir, &nested_archive).unwrap();
            assert!(digest.starts_with("sha256:"));
            assert!(nested_archive.exists());
        }

        #[test]
        fn returns_error_for_nonexistent_source() {
            let temp_dir = TempDir::new().unwrap();
            let nonexistent = temp_dir.path().join("nonexistent");
            let archive_path = temp_dir.path().join("archive.tgz");

            let result = create_feature_tgz(&nonexistent, &archive_path);
            assert!(result.is_err());
        }
    }
}
