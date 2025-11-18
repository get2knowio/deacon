use deacon_core::lockfile::{Lockfile, LockfileFeature};
use deacon_core::outdated::{
    canonical_feature_id, compute_wanted_version, derive_current_version, latest_major,
    wanted_major,
};
use std::collections::HashMap;

#[test]
fn test_wanted_and_wanted_major() {
    let reference = "ghcr.io/devcontainers/features/node:v1.2.3";
    // compute_wanted_version should strip leading 'v'
    let wanted = compute_wanted_version(reference).expect("expected wanted");
    assert_eq!(wanted, "1.2.3");

    // wanted_major should parse major portion
    let maj = wanted_major(&Some(wanted.clone()));
    assert_eq!(maj.as_deref(), Some("1"));
}

#[test]
fn test_latest_major() {
    let latest = Some("2.5.0".to_string());
    let maj = latest_major(&latest);
    assert_eq!(maj.as_deref(), Some("2"));
}

#[test]
fn test_derive_current_version_from_lockfile() {
    let mut lf = Lockfile {
        features: HashMap::new(),
    };
    lf.features.insert(
        "ghcr.io/devcontainers/features/node".to_string(),
        LockfileFeature {
            version: "7.8.9".to_string(),
            resolved: "ghcr.io/devcontainers/features/node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            integrity: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            depends_on: None,
        },
    );

    // When lockfile has entry, derive_current_version should return the locked version
    let current = derive_current_version("ghcr.io/devcontainers/features/node:10.0.0", Some(&lf));
    assert_eq!(current.as_deref(), Some("7.8.9"));

    // When lockfile is absent, it should fall back to the wanted tag
    let current_no_lock =
        derive_current_version("ghcr.io/devcontainers/features/node:10.0.0", None);
    assert_eq!(current_no_lock.as_deref(), Some("10.0.0"));

    // Digest-based reference with no lockfile should yield None
    let digest_ref = "ghcr.io/devcontainers/features/node@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let current_digest = derive_current_version(digest_ref, None);
    assert!(current_digest.is_none());
}

#[test]
fn test_canonical_feature_id_with_port_and_tag() {
    let input = "localhost:5000/owner/feat:1.0";
    assert_eq!(canonical_feature_id(input), "localhost:5000/owner/feat");
}
