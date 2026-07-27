//! `chan-container-state` observer + normalization (024 Phase 4).
//!
//! Hermetic: the observer reads the runner's PRE-FETCHED `docker inspect` object, so a
//! recorded inspect document exercises the whole path — capture, delegation to the shared
//! `normalize::container_state`, the derived `workspaceBindTargets`, the per-channel rule
//! chain, and the differential verdict — with no Docker, no network and no oracle.
//!
//! Two properties here are load-bearing enough that each test also demonstrates what
//! happens WITHOUT them (FR-047):
//!
//! - `workspace_basename_token`: without it, two sides in different temp workspaces
//!   diverge on a mount key that is an artifact of the runner's own isolation;
//! - verbatim labels: without the retired `strip_intentional_labels` drop, a per-CLI
//!   identity label difference is VISIBLE and must be characterized on the case — the
//!   whole point of retiring a rule that silently removed a label namespace.

use parity_harness::exec::Side;
use std::collections::HashSet;
use std::path::Path;

use deacon_conformance::model::{CHAN_CONTAINER_STATE, OBSERVED_CHANNELS, Operation};
use parity_harness::compare::{Tolerances, verdict_differential};
use parity_harness::evidence::Outcome;
use parity_harness::normalize::{TokenMap, normalize_channel, tokens_for_channel};
use parity_harness::observe::container_state::ContainerStateObserver;
use parity_harness::observe::{ChannelObserver, RunContext, observer_for};
use serde_json::{Value, json};

/// A recorded `docker inspect` object for a container one CLI created in `workspace`,
/// with `labels` merged over the identity labels that CLI stamps. Everything else is
/// identical between the two sides, so any divergence a test sees is the property under
/// test and nothing else.
fn recorded_inspect(workspace: &str, name: &str, labels: Value) -> Value {
    let mut all = json!({
        "org.opencontainers.image.title": "demo",
    });
    if let (Some(dst), Some(src)) = (all.as_object_mut(), labels.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    json!({
        "Config": {
            "User": "vscode",
            "WorkingDir": format!("/workspaces/{name}"),
            "Env": ["PATH=/usr/bin", "HOME=/root", "FOO=bar"],
            "Labels": all,
            "ExposedPorts": { "3000/tcp": {} },
            "Entrypoint": ["/bin/sh", "-c", "sleep infinity"],
            "Cmd": ["-c", "echo hi"],
        },
        "HostConfig": {
            "PortBindings": { "3000/tcp": [{ "HostIp": "", "HostPort": "3000" }] }
        },
        "NetworkSettings": { "Networks": { "bridge": {} } },
        "Mounts": [
            {
                "Type": "bind",
                "Source": workspace,
                "Destination": format!("/workspaces/{name}"),
                "RW": true
            },
            {
                "Type": "volume",
                "Name": "feat-probe-vol",
                "Source": "/var/lib/docker/volumes/x/_data",
                "Destination": "/feat-mnt",
                "RW": false
            }
        ]
    })
}

fn op() -> Operation {
    Operation {
        id: "op-up".to_string(),
        subcommand: "up".to_string(),
        ..Operation::default()
    }
}

/// Capture the channel for a side whose isolated workspace is `/tmp/<name>`.
fn capture(name: &str, labels: Value) -> parity_harness::evidence::RawChannelEvidence {
    let workspace = format!("/tmp/{name}");
    let mut ctx = RunContext::new(std::path::PathBuf::from(&workspace));
    ctx.container_inspect = Some(recorded_inspect(&workspace, name, labels));
    ContainerStateObserver
        .capture(&ctx, &op())
        .expect("capture succeeds over a well-formed inspect")
}

/// The normalized evidence for a side, under the channel's real token policy.
fn normalized(name: &str, labels: Value) -> parity_harness::evidence::NormalizedChannelEvidence {
    let raw = capture(name, labels);
    let tokens = tokens_for_channel(CHAN_CONTAINER_STATE, Path::new(&format!("/tmp/{name}")));
    normalize_channel(CHAN_CONTAINER_STATE, &raw, &tokens, Side::Deacon)
}

fn differ(
    a: &parity_harness::evidence::NormalizedChannelEvidence,
    b: &parity_harness::evidence::NormalizedChannelEvidence,
) -> parity_harness::evidence::ChannelVerdict {
    let mut consumed = HashSet::new();
    verdict_differential(
        CHAN_CONTAINER_STATE,
        a,
        b,
        &Tolerances::new(&[], &[]),
        &mut consumed,
    )
}

// ---------------------------------------------------------------------------------
// Field mapping + delegation.
// ---------------------------------------------------------------------------------

#[test]
fn the_channel_is_wired_and_observable() {
    assert!(observer_for(CHAN_CONTAINER_STATE).is_some());
    assert!(OBSERVED_CHANNELS.contains(&CHAN_CONTAINER_STATE));
    assert_eq!(ContainerStateObserver.channel(), CHAN_CONTAINER_STATE);
}

#[test]
fn emits_every_declared_field_from_the_delegated_snapshot() {
    let ev = capture("ws-a", json!({}));
    assert!(ev.present);
    let v = &ev.value;
    for field in [
        "mounts",
        "env",
        "labels",
        "user",
        "workingDir",
        "exposedPorts",
        "publishedPorts",
        "entrypoint",
        "cmd",
        "networks",
        "workspaceBindTargets",
    ] {
        assert!(
            v.get(field).is_some(),
            "`{field}` must be emitted; got {v:#}"
        );
    }

    // The values come from the SHARED normalizer, not a second derivation here.
    assert_eq!(v["user"], json!("vscode"));
    assert_eq!(v["workingDir"], json!("/workspaces/ws-a"));
    assert_eq!(v["exposedPorts"], json!(["3000/tcp"]));
    assert_eq!(v["publishedPorts"], json!(["3000/tcp"]));
    assert_eq!(v["entrypoint"], json!(["/bin/sh", "-c", "sleep infinity"]));
    assert_eq!(v["cmd"], json!(["-c", "echo hi"]));
    assert_eq!(v["networks"], json!(["bridge"]));

    // Mounts are keyed by DESTINATION, carrying the delegated mount state.
    assert_eq!(v["mounts"]["/feat-mnt"]["mountType"], json!("volume"));
    assert_eq!(
        v["mounts"]["/feat-mnt"]["ro"],
        json!(true),
        "a read-only mount is observed as such"
    );
    assert_eq!(
        v["mounts"]["/feat-mnt"]["sourceTail"],
        json!("feat-probe-vol")
    );

    // 024 US5 (T123): `drop_noise_env` no longer runs at CAPTURE, so every variable
    // reaches the channel — `PATH` included, which FR-050 requires be compared.
    assert_eq!(v["env"], json!(["FOO=bar", "HOME=/root", "PATH=/usr/bin"]));

    // The US5 derived fields (T122) accompany the raw ones; each turns a comparison that
    // would otherwise need a search into ordinary equality.
    assert_eq!(v["envMap"]["PATH"], json!("/usr/bin"));
    assert_eq!(
        v["pathSegments"],
        json!(["/usr/bin"]),
        "PATH compares segment-wise, so a Feature-contributed segment is an array subset"
    );
    assert_eq!(
        v["mountSources"]["/feat-mnt"],
        json!("feat-probe-vol"),
        "the WHOLE source, not the leaf `sourceTail` collapses to"
    );
    assert_eq!(
        v["userSpec"],
        json!({ "name": "vscode", "uid": null, "group": null, "gid": null })
    );
    assert_eq!(v["composeProjectResources"]["networks"], json!(["bridge"]));
    assert!(
        v["labelNamespaces"].is_object(),
        "labels are also grouped by namespace (FR-052)"
    );
}

#[test]
fn entrypoint_cmd_and_networks_are_emitted_so_they_can_be_compared() {
    // The legacy comparison documented these as "captured but NOT diffed" (#290) — an
    // undeclared non-comparison, invisible to anyone reading a case. They are emitted
    // here, so a difference SHOWS UP and has to be declared on the case instead.
    let deacon = normalized("ws-a", json!({}));
    let mut reference = normalized("ws-b", json!({}));
    reference.value["entrypoint"] = json!(["/bin/sh", "-c", "exec \"$@\""]);

    let v = differ(&deacon, &reference);
    assert_eq!(
        v.outcome,
        Outcome::Diverge,
        "a differing entrypoint must be VISIBLE, not silently not-diffed: {v:?}"
    );
    let detail = v.detail.expect("detail").to_string();
    assert!(
        detail.contains("chan-container-state.entrypoint"),
        "the diverging path names the field: {detail}"
    );
}

// ---------------------------------------------------------------------------------
// Derived `workspaceBindTargets` (the derived-field-not-a-query-language rule).
// ---------------------------------------------------------------------------------

#[test]
fn workspace_bind_targets_is_the_default_mount_target_claim_as_plain_equality() {
    // `default-workspace-mount-target-parity` asserts: ∄ a mount at `/workspace` ∧ ∃ a
    // bind under `/workspaces/*` rooted at the workspace. As a derived field that is a
    // LIST, so the cross-CLI claim is ordinary equality — no quantified map predicate.
    let deacon = normalized("ws-a", json!({}));
    let reference = normalized("ws-b", json!({}));

    assert_eq!(
        deacon.value["workspaceBindTargets"],
        json!(["/workspaces/<WORKSPACE_NAME>"]),
        "exactly one workspace-rooted bind, under /workspaces/*"
    );
    assert_eq!(
        deacon.value["workspaceBindTargets"], reference.value["workspaceBindTargets"],
        "the two CLIs agree on the target — a plain equality, not a search"
    );
    assert!(
        !deacon.value["mounts"]
            .as_object()
            .expect("mounts object")
            .contains_key("/workspace"),
        "and there is no mount at the singular /workspace"
    );

    // A CLI that mounted the workspace at the WRONG target is caught by the same
    // equality — the claim has real teeth.
    let mut wrong = reference.clone();
    wrong.value["workspaceBindTargets"] = json!(["/workspace"]);
    assert_eq!(
        differ(&deacon, &wrong).outcome,
        Outcome::Diverge,
        "a /workspace target must diverge from /workspaces/<name>"
    );
}

#[test]
fn only_binds_rooted_at_the_workspace_are_derived_targets() {
    let workspace = "/tmp/ws-a";
    let mut ctx = RunContext::new(std::path::PathBuf::from(workspace));
    ctx.container_inspect = Some(json!({
        "Config": { "User": "", "Labels": {} },
        "Mounts": [
            { "Type": "bind", "Source": workspace, "Destination": "/workspaces/ws-a", "RW": true },
            { "Type": "bind", "Source": "/tmp/other", "Destination": "/elsewhere", "RW": true },
            { "Type": "volume", "Name": "v", "Source": workspace, "Destination": "/vol", "RW": true }
        ]
    }));
    let ev = ContainerStateObserver
        .capture(&ctx, &op())
        .expect("capture");
    assert_eq!(
        ev.value["workspaceBindTargets"],
        json!(["/workspaces/ws-a"]),
        "a bind elsewhere and a volume are not workspace bind targets: {:#}",
        ev.value
    );
}

// ---------------------------------------------------------------------------------
// `workspace_basename_token` — and what happens without it (FR-047).
// ---------------------------------------------------------------------------------

#[test]
fn two_different_temp_workspaces_compare_equal() {
    let deacon = normalized("ws-a", json!({}));
    let reference = normalized("ws-b", json!({}));
    let v = differ(&deacon, &reference);
    assert_eq!(
        v.outcome,
        Outcome::Agree,
        "the per-side temp workspace name must NOT be a divergence: {v:?}"
    );
}

#[test]
fn without_the_basename_token_the_same_evidence_diverges() {
    // The demonstration that `workspace_basename_token` is load-bearing: normalize the
    // SAME two captures with the plain full-path token map (what every other channel
    // uses) and the comparison reports a divergence that is purely an artifact of the
    // isolation the runner imposes.
    let plain = |name: &str| {
        let raw = capture(name, json!({}));
        normalize_channel(
            CHAN_CONTAINER_STATE,
            &raw,
            &TokenMap::workspace(Path::new(&format!("/tmp/{name}"))),
            Side::Deacon,
        )
    };
    let v = differ(&plain("ws-a"), &plain("ws-b"));
    assert_eq!(
        v.outcome,
        Outcome::Diverge,
        "without the basename token the two sides differ on container-side paths"
    );
    let detail = v.detail.expect("detail").to_string();
    assert!(
        detail.contains("workingDir") || detail.contains("mounts"),
        "the artifact shows up on the workspace-derived paths: {detail}"
    );
}

// ---------------------------------------------------------------------------------
// Labels verbatim — and what the retired drop used to hide (FR-047).
// ---------------------------------------------------------------------------------

#[test]
fn every_label_is_emitted_including_the_cli_namespaced_ones() {
    let ev = capture(
        "ws-a",
        json!({
            "devcontainer.local_folder": "/tmp/ws-a",
            "devcontainer.configHash": "abc123",
            "com.docker.compose.project": "deacon_1_2",
            "desktop.docker.io/binds/0/Source": "/tmp/ws-a",
            "dev.containers.id": "zzz"
        }),
    );
    let labels = ev.value["labels"].as_object().expect("labels object");
    for key in [
        "devcontainer.local_folder",
        "devcontainer.configHash",
        "com.docker.compose.project",
        "desktop.docker.io/binds/0/Source",
        "dev.containers.id",
        "org.opencontainers.image.title",
    ] {
        assert!(
            labels.contains_key(key),
            "`{key}` must survive capture verbatim (the four-prefix drop is retired); \
             got {labels:#?}"
        );
    }
}

#[test]
fn a_per_cli_identity_label_difference_is_visible_and_must_be_characterized() {
    // The old `strip_intentional_labels` rule removed these four NAMESPACES, so this
    // comparison silently agreed — including for any label a future release adds under
    // them. Now it diverges, which is the honest state: the difference is real, and a
    // case that tolerates it must SAY SO with a scoped, backed allowed-difference.
    let deacon = normalized(
        "ws-a",
        json!({ "devcontainer.configHash": "abc123", "com.docker.compose.project": "deacon_1_2" }),
    );
    let reference = normalized(
        "ws-b",
        json!({ "devcontainer.metadata": "[{}]", "com.docker.compose.project": "ws-b_devcontainer" }),
    );

    let v = differ(&deacon, &reference);
    assert_eq!(
        v.outcome,
        Outcome::Diverge,
        "identity labels differ between the CLIs and that is now VISIBLE: {v:?}"
    );
    let detail = v.detail.expect("detail").to_string();
    assert!(
        detail.contains("chan-container-state.labels"),
        "the diverging path names the label field so a tolerance can scope to it: {detail}"
    );

    // And a scoped tolerance — the mechanism that REPLACES the blanket drop — turns it
    // into an `allowed-difference` while every other field stays compared.
    // Two paths, because 024 US5's derived `labelNamespaces` reports the same one-sided
    // label a SECOND way (as a difference in the namespace's membership) and the compose
    // project name reaches the derived `composeProjectResources`. Each is named
    // explicitly: a tolerance that covered them implicitly would be the blanket ignore
    // this mechanism replaces.
    let path = |p: &str| deacon_conformance::model::AllowedDifference {
        behavior: "bhv-x".to_string(),
        context: vec![],
        observable_path: p.to_string(),
        rationale: "each CLI stamps its own identity/bookkeeping labels".to_string(),
        waiver_id: None,
        divergence_id: Some("ext-container-identity-labels".to_string()),
    };
    let allowed = vec![
        path("chan-container-state.labels"),
        path("chan-container-state.labelNamespaces"),
        path("chan-container-state.composeProjectResources.project"),
    ];
    let behaviors = vec!["bhv-x".to_string()];
    let tolerances = Tolerances::new(&allowed, &behaviors);
    let mut consumed = HashSet::new();
    let tolerated = verdict_differential(
        CHAN_CONTAINER_STATE,
        &deacon,
        &reference,
        &tolerances,
        &mut consumed,
    );
    assert_eq!(
        tolerated.outcome,
        Outcome::AllowedDifference,
        "a scoped, backed tolerance covers exactly the label path: {tolerated:?}"
    );
    assert!(
        !consumed.is_empty(),
        "the tolerance is consumed, so it is stale-checked like every other"
    );
}
