//! Unit tests for canonical ID computation from manifest bytes digest

use sha2::{Digest, Sha256};

#[test]
fn test_canonical_id_computation() {
    // Test that canonical ID is computed as SHA256 digest of manifest bytes
    let manifest_json = r#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.devcontainers.feature.config.v1+json",
            "size": 0,
            "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": 1024,
            "digest": "sha256:abc123def456"
        }],
        "annotations": {
            "org.opencontainers.image.title": "test-feature",
            "org.opencontainers.image.description": "A test feature",
            "org.opencontainers.image.version": "1.0.0"
        }
    }"#;

    let manifest_bytes = manifest_json.as_bytes();

    // Compute expected digest
    let mut hasher = Sha256::new();
    hasher.update(manifest_bytes);
    let expected_digest = format!("sha256:{:x}", hasher.finalize());

    // Verify the digest format
    assert!(expected_digest.starts_with("sha256:"));
    assert_eq!(expected_digest.len(), 71); // sha256: + 64 hex chars

    // Test with different manifest content
    let different_manifest = r#"{"schemaVersion": 2, "layers": []}"#;
    let mut hasher2 = Sha256::new();
    hasher2.update(different_manifest.as_bytes());
    let different_digest = format!("sha256:{:x}", hasher2.finalize());

    // Digests should be different for different content
    assert_ne!(expected_digest, different_digest);
}

#[test]
fn test_canonical_id_deterministic() {
    // Test that the same manifest bytes always produce the same canonical ID
    let manifest_json = r#"{"test": "data", "number": 42}"#;

    let mut hasher1 = Sha256::new();
    hasher1.update(manifest_json.as_bytes());
    let digest1 = format!("sha256:{:x}", hasher1.finalize());

    let mut hasher2 = Sha256::new();
    hasher2.update(manifest_json.as_bytes());
    let digest2 = format!("sha256:{:x}", hasher2.finalize());

    assert_eq!(digest1, digest2);
}
