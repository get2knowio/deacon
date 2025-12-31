use deacon_core::lockfile::{Lockfile, LockfileFeature};
use deacon_core::outdated::{
    canonical_feature_id, compute_wanted_version, derive_current_version, latest_major,
    wanted_major,
};
use std::collections::HashMap;

#[test]
fn test_canonical_feature_id_digest_and_port() {
    let input = "registry.example.com:5000/owner/feature@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert_eq!(
        canonical_feature_id(input),
        "registry.example.com:5000/owner/feature"
    );
}

#[test]
fn test_compute_wanted_version_various_tags() {
    let t1 = "ghcr.io/devcontainers/features/node:v18";
    assert_eq!(compute_wanted_version(t1).as_deref(), Some("18"));

    let t2 = "ghcr.io/devcontainers/features/node:1.2.3";
    assert_eq!(compute_wanted_version(t2).as_deref(), Some("1.2.3"));

    let t3 = "ghcr.io/devcontainers/features/node@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    assert!(compute_wanted_version(t3).is_none());
}

#[test]
fn test_wanted_and_latest_major_none_handling() {
    assert_eq!(wanted_major(&None), None);
    assert_eq!(latest_major(&None), None);
    assert_eq!(
        wanted_major(&Some("3.4.5".to_string())).as_deref(),
        Some("3")
    );
    assert_eq!(
        latest_major(&Some("3.4.5".to_string())).as_deref(),
        Some("3")
    );
}

#[test]
fn test_derive_current_version_lockfile_behavior() {
    let mut lf = Lockfile {
        features: HashMap::new(),
    };
    lf.features.insert(
        "ghcr.io/devcontainers/features/sample".to_string(),
        LockfileFeature {
            version: "0.1.2".to_string(),
            resolved: "ghcr.io/devcontainers/features/sample@sha256:ccc".to_string(),
            integrity: "sha256:ccc".to_string(),
            depends_on: None,
        },
    );

    // Derive from lockfile
    let current = derive_current_version("ghcr.io/devcontainers/features/sample:9.9.9", Some(&lf));
    assert_eq!(current.as_deref(), Some("0.1.2"));

    // No lockfile -> fallback to wanted
    let current_no_lock =
        derive_current_version("ghcr.io/devcontainers/features/sample:9.9.9", None);
    assert_eq!(current_no_lock.as_deref(), Some("9.9.9"));
}
