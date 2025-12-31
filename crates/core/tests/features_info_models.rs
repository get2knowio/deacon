//! Unit tests for Features Info data contracts and serialization

use deacon_core::features_info::{ManifestJson, PublishedTagsJson, VerboseJson};
use serde_json::json;

#[test]
fn test_manifest_json_roundtrip() {
    let manifest = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": 1024,
            "digest": "sha256:abc123"
        }]
    });

    let original = ManifestJson {
        manifest: manifest.clone(),
        canonical_id: Some("sha256:def456".to_string()),
    };

    // Serialize to JSON
    let json_str = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: ManifestJson = serde_json::from_str(&json_str).unwrap();

    // Verify fields
    assert_eq!(deserialized.canonical_id, Some("sha256:def456".to_string()));
    assert_eq!(deserialized.manifest["schemaVersion"], 2);
    assert_eq!(deserialized.manifest["layers"][0]["size"], 1024);
}

#[test]
fn test_published_tags_json_roundtrip() {
    let tags = vec![
        "v1.0.0".to_string(),
        "v1.1.0".to_string(),
        "latest".to_string(),
    ];

    let original = PublishedTagsJson {
        published_tags: tags.clone(),
    };

    // Serialize to JSON
    let json_str = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: PublishedTagsJson = serde_json::from_str(&json_str).unwrap();

    // Verify tags are preserved
    assert_eq!(deserialized.published_tags, tags);
}

#[test]
fn test_verbose_json_roundtrip() {
    let manifest = json!({
        "schemaVersion": 2,
        "layers": []
    });

    let mut errors = std::collections::HashMap::new();
    errors.insert("tags".to_string(), "timeout".to_string());

    let original = VerboseJson {
        manifest: manifest.clone(),
        canonical_id: Some("sha256:abc123".to_string()),
        published_tags: vec!["v1.0".to_string()],
        errors: errors.clone(),
    };

    // Serialize to JSON
    let json_str = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: VerboseJson = serde_json::from_str(&json_str).unwrap();

    // Verify all fields
    assert_eq!(deserialized.canonical_id, Some("sha256:abc123".to_string()));
    assert_eq!(deserialized.published_tags, vec!["v1.0".to_string()]);
    assert_eq!(deserialized.errors, errors);
    assert_eq!(deserialized.manifest["schemaVersion"], 2);
}

#[test]
fn test_verbose_json_optional_fields() {
    let manifest = json!({"test": true});

    // Test with all optional fields present
    let with_all = VerboseJson {
        manifest: manifest.clone(),
        canonical_id: Some("sha256:test".to_string()),
        published_tags: vec!["tag1".to_string()],
        errors: [("err".to_string(), "msg".to_string())].into(),
    };

    let json_all = serde_json::to_value(&with_all).unwrap();
    let obj_all = json_all.as_object().unwrap();
    assert!(obj_all.contains_key("canonical_id"));
    assert!(obj_all.contains_key("published_tags"));
    assert!(obj_all.contains_key("errors"));

    // Test with no optional fields
    let with_none = VerboseJson {
        manifest: manifest.clone(),
        canonical_id: None,
        published_tags: vec![],
        errors: std::collections::HashMap::new(),
    };

    let json_none = serde_json::to_value(&with_none).unwrap();
    let obj_none = json_none.as_object().unwrap();
    assert!(!obj_none.contains_key("canonical_id"));
    assert!(!obj_none.contains_key("published_tags"));
    assert!(!obj_none.contains_key("errors"));
}

#[test]
fn test_manifest_json_null_canonical_id() {
    let manifest = json!({"schemaVersion": 2});

    let original = ManifestJson {
        manifest,
        canonical_id: None,
    };

    let json_value = serde_json::to_value(&original).unwrap();
    let obj = json_value.as_object().unwrap();

    // canonical_id should not be present when None
    assert!(!obj.contains_key("canonical_id"));
}
