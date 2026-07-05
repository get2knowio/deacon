//! Pure-logic unit tests for the normalized observable-state differ
//! (`parity_utils::{snapshot_from_inspect, diff_states, classify_divergence,
//! assert_snapshots_parity}`). No Docker, no upstream CLI — feeds fixed
//! `docker inspect` JSON through the differ so its correctness (and teeth) are
//! covered in the fast test loop, independent of the gated Docker fixtures in
//! `parity_state_diff.rs`.

use serde_json::json;

mod parity_utils;
use parity_utils::{
    DivergenceClass, StateSnapshot, assert_snapshots_parity, classify_divergence, diff_states,
    snapshot_from_inspect,
};

/// A minimal single-container `docker inspect` object with the given env,
/// mounts, and labels.
fn inspect(
    env: &[&str],
    mounts: serde_json::Value,
    labels: serde_json::Value,
) -> serde_json::Value {
    json!({
        "Config": {
            "Env": env,
            "Labels": labels,
            "User": "",
            "WorkingDir": "/workspace",
            "ExposedPorts": {},
            "Entrypoint": null,
            "Cmd": ["sleep", "infinity"],
        },
        "Mounts": mounts,
        "NetworkSettings": { "Networks": { "bridge": {} } },
    })
}

fn bind(dest: &str, source: &str) -> serde_json::Value {
    json!({ "Type": "bind", "Source": source, "Destination": dest, "RW": true })
}

fn volume(dest: &str, name: &str) -> serde_json::Value {
    json!({ "Type": "volume", "Name": name, "Source": "/var/lib/docker/volumes/x/_data", "Destination": dest, "RW": true })
}

#[test]
fn snapshot_strips_noise_env_and_cli_labels() {
    let raw = inspect(
        &["PATH=/usr/bin", "HOSTNAME=abc", "FOO=bar", "BAZ=qux"],
        json!([bind("/workspace", "/tmp/ws-a")]),
        json!({
            "devcontainer.source": "deacon",
            "devcontainer.metadata": "[...]",
            "com.docker.compose.project": "deacon_1_2",
            "org.opencontainers.image.title": "debian",
        }),
    );
    let snap = snapshot_from_inspect(&raw);
    // Noise env removed; real env kept.
    assert!(snap.env.contains("FOO=bar"));
    assert!(snap.env.contains("BAZ=qux"));
    assert!(!snap.env.iter().any(|e| e.starts_with("PATH=")));
    assert!(!snap.env.iter().any(|e| e.starts_with("HOSTNAME=")));
    // CLI-namespaced labels stripped; semantic image label kept.
    assert!(!snap.labels.contains_key("devcontainer.source"));
    assert!(!snap.labels.contains_key("com.docker.compose.project"));
    assert_eq!(
        snap.labels
            .get("org.opencontainers.image.title")
            .map(String::as_str),
        Some("debian")
    );
    // Mount captured by destination.
    assert_eq!(
        snap.mounts.get("/workspace").map(|m| m.mount_type.as_str()),
        Some("bind")
    );
}

#[test]
fn snapshot_normalizes_compose_project_prefix_on_volumes() {
    let raw = inspect(
        &["FOO=bar"],
        json!([volume("/feat-mnt", "deacon_1_2_feat-probe-vol")]),
        json!({ "com.docker.compose.project": "deacon_1_2" }),
    );
    let snap = snapshot_from_inspect(&raw);
    // The project prefix is stripped from the reporting source tail so it is
    // comparable to upstream's differently-prefixed volume name.
    assert_eq!(
        snap.mounts.get("/feat-mnt").map(|m| m.source_tail.as_str()),
        Some("feat-probe-vol")
    );
}

#[test]
fn identical_snapshots_have_no_divergences() {
    let a = snapshot_from_inspect(&inspect(
        &["FOO=bar"],
        json!([bind("/workspace", "/tmp/ws-a")]),
        json!({}),
    ));
    let b = snapshot_from_inspect(&inspect(
        &["FOO=bar"],
        // Different bind SOURCE (per-workspace temp path) must NOT be a divergence.
        json!([bind("/workspace", "/tmp/ws-b")]),
        json!({}),
    ));
    assert!(diff_states(&a, &b).is_empty(), "{:?}", diff_states(&a, &b));
}

#[test]
fn missing_mount_is_detected() {
    let deacon = snapshot_from_inspect(&inspect(
        &["FOO=bar"],
        json!([bind("/workspace", "/tmp/ws-a")]),
        json!({}),
    ));
    let upstream = snapshot_from_inspect(&inspect(
        &["FOO=bar"],
        json!([bind("/workspace", "/tmp/ws-b"), volume("/data", "up_data")]),
        json!({}),
    ));
    let divs = diff_states(&deacon, &upstream);
    assert_eq!(divs.len(), 1, "{divs:?}");
    assert_eq!(divs[0].field, "mount:/data");
    assert!(divs[0].detail.contains("absent deacon"));
}

#[test]
fn missing_env_is_detected() {
    let deacon = snapshot_from_inspect(&inspect(&["FOO=bar"], json!([]), json!({})));
    let upstream = snapshot_from_inspect(&inspect(&["FOO=bar", "SECRET=1"], json!([]), json!({})));
    let divs = diff_states(&deacon, &upstream);
    assert!(divs.iter().any(|d| d.field == "env:SECRET"));
}

#[test]
fn feature_mount_gap_classifies_as_known_gap_272() {
    // The #272 gap: upstream has the feature volume at /feat-mnt, deacon lacks it.
    match classify_divergence("mount:/feat-mnt", &[]) {
        DivergenceClass::KnownGap(gap) => assert_eq!(gap.issue, "#272"),
        _ => panic!("mount:/feat-mnt should classify as the #272 known gap"),
    }
}

#[test]
fn unlisted_divergence_classifies_as_unexpected() {
    assert!(matches!(
        classify_divergence("mount:/some-other", &[]),
        DivergenceClass::Unexpected
    ));
    // ...unless the caller explicitly allows it.
    assert!(matches!(
        classify_divergence("mount:/some-other", &["mount:/some-other"]),
        DivergenceClass::Intentional(_)
    ));
}

#[test]
fn assert_snapshots_parity_passes_on_known_gap_but_fails_on_new_divergence() {
    let base = snapshot_from_inspect(&inspect(&["FOO=bar"], json!([]), json!({})));

    // Only a known-gap divergence (/feat-mnt) → passes.
    let with_gap = snapshot_from_inspect(&inspect(
        &["FOO=bar"],
        json!([volume("/feat-mnt", "up_feat-probe-vol")]),
        json!({}),
    ));
    assert_snapshots_parity(&base, &with_gap, &[]);

    // A NEW, unlisted divergence → must panic (teeth).
    let with_new = snapshot_from_inspect(&inspect(
        &["FOO=bar"],
        json!([volume("/unexpected", "up_vol")]),
        json!({}),
    ));
    let res = std::panic::catch_unwind(|| assert_snapshots_parity(&base, &with_new, &[]));
    assert!(
        res.is_err(),
        "an unlisted divergence must fail the parity assertion"
    );

    // ...but the caller can allow it explicitly.
    assert_snapshots_parity(&base, &with_new, &["mount:/unexpected"]);
}

#[test]
fn allowlist_matcher_is_exact_by_default_and_prefix_with_star() {
    // Exact matcher must NOT swallow a longer path that shares its prefix:
    // allowing `mount:/workspace` must not also allow `mount:/workspaces/sib`.
    assert!(matches!(
        classify_divergence("mount:/workspaces/sib", &["mount:/workspace"]),
        DivergenceClass::Unexpected
    ));
    assert!(matches!(
        classify_divergence("mount:/workspace", &["mount:/workspace"]),
        DivergenceClass::Intentional(_)
    ));
    // A trailing `*` opts into prefix matching.
    assert!(matches!(
        classify_divergence("mount:/workspaces/sib", &["mount:/workspaces*"]),
        DivergenceClass::Intentional(_)
    ));
}

#[test]
fn empty_user_equals_root() {
    let a = snapshot_from_inspect(&json!({
        "Config": { "Env": ["FOO=bar"], "User": "" },
        "Mounts": [], "NetworkSettings": { "Networks": {} }
    }));
    let b = snapshot_from_inspect(&json!({
        "Config": { "Env": ["FOO=bar"], "User": "root" },
        "Mounts": [], "NetworkSettings": { "Networks": {} }
    }));
    assert!(
        diff_states(&a, &b).is_empty(),
        "empty User and root must be equivalent: {:?}",
        diff_states(&a, &b)
    );
    // ...but a real non-root user still diverges.
    let c = snapshot_from_inspect(&json!({
        "Config": { "Env": ["FOO=bar"], "User": "node" },
        "Mounts": [], "NetworkSettings": { "Networks": {} }
    }));
    assert!(diff_states(&a, &c).iter().any(|d| d.field == "user"));
}

#[test]
fn published_ports_are_captured_and_diffed() {
    let with_port = json!({
        "Config": { "Env": ["FOO=bar"], "ExposedPorts": {} },
        "Mounts": [], "NetworkSettings": { "Networks": {} },
        "HostConfig": { "PortBindings": { "3000/tcp": [{ "HostIp": "", "HostPort": "3000" }] } }
    });
    let no_port = json!({
        "Config": { "Env": ["FOO=bar"], "ExposedPorts": {} },
        "Mounts": [], "NetworkSettings": { "Networks": {} },
        "HostConfig": { "PortBindings": {} }
    });
    let a = snapshot_from_inspect(&with_port);
    assert!(a.published_ports.contains("3000/tcp"));
    let b = snapshot_from_inspect(&no_port);
    let divs = diff_states(&a, &b);
    assert!(
        divs.iter().any(|d| d.field == "pubport:3000/tcp"),
        "a published-port difference must be detected: {divs:?}"
    );
    // Same ports on both → no divergence.
    assert!(diff_states(&a, &a).is_empty());
}

#[test]
fn default_snapshot_yields_no_divergences_against_itself() {
    let s = StateSnapshot::default();
    assert!(diff_states(&s, &s).is_empty());
}
