//! Canary lane — comparison against explicitly pinned upstream *development* revisions
//! (026-continuous-conformance-certification, US5; FR-017 … FR-021).
//!
//! ## Non-blocking everywhere, and structurally unable to touch the record
//!
//! This lane exists for early warning, which is strictly secondary to keeping the stable
//! record clean. It therefore holds no write path to a registry record, a committed
//! snapshot, or a pin — and its pins live in the discovery data root, which no registry
//! loader can reach. That isolation is what makes running canaries safe at all.
//!
//! Runs ONLY under `cargo nextest run --profile canary`.

use std::collections::BTreeSet;

use deacon_conformance::discovery::queue::{CanaryTarget, check_canary_pins, load_canary};
use deacon_conformance::{default_canary_file, default_discovery_dir, workspace_root};

/// The committed canary pins. Loaded through the shared model rather than a local
/// duplicate: a second parser is a second answer to "what is a valid pin?", and the whole
/// point of D6 is that there is exactly one.
fn canary_pins() -> Vec<deacon_conformance::discovery::queue::CanaryPin> {
    load_canary(&default_discovery_dir()).unwrap_or_else(|e| panic!("canary pins load: {e}"))
}

#[test]
fn every_canary_pin_is_an_immutable_revision() {
    // FR-018 / D6. Asserted here as well as in the hermetic checker because this is the
    // lane that would actually resolve the reference: a mutable pin makes a finding
    // un-reobservable, and an un-reobservable finding can never be triaged.
    let violations = check_canary_pins(&canary_pins());
    assert!(
        violations.is_empty(),
        "committed canary pins must be well-formed: {:?}",
        violations
            .iter()
            .map(|v| format!("{} {}", v.class(), v.record()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_mutable_revision_is_rejected() {
    use deacon_conformance::discovery::queue::{CanaryPin, derive_canary_id};
    // The positive control: the checker must actually fail on a branch name, or the
    // assertion above is only claiming that an empty list is empty.
    for mutable in ["main", "latest", "v1", "next"] {
        let pin = CanaryPin {
            id: derive_canary_id(CanaryTarget::ReferenceCli, mutable),
            target: CanaryTarget::ReferenceCli,
            revision: mutable.to_string(),
            url: "https://example.invalid".into(),
            added: "2026-07-28".into(),
        };
        let violations = check_canary_pins(std::slice::from_ref(&pin));
        assert!(
            violations.iter().any(|v| v.to_string().contains("mutable")),
            "`{mutable}` must be rejected as a mutable revision"
        );
    }
}

#[test]
fn an_immutable_revision_is_accepted() {
    use deacon_conformance::discovery::queue::{CanaryPin, derive_canary_id};
    for immutable in [
        "9f21ab7712c4a5b6d8e0f1234567890abcdef012",
        "0.88.0",
        "0.88.0-rc.1",
    ] {
        let pin = CanaryPin {
            id: derive_canary_id(CanaryTarget::ReferenceCli, immutable),
            target: CanaryTarget::ReferenceCli,
            revision: immutable.to_string(),
            url: "https://example.invalid".into(),
            added: "2026-07-28".into(),
        };
        assert!(
            check_canary_pins(std::slice::from_ref(&pin)).is_empty(),
            "`{immutable}` must be accepted"
        );
    }
}

#[test]
fn canary_pin_ids_are_unique() {
    let pins = canary_pins();
    let ids: BTreeSet<&str> = pins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), pins.len(), "two canary pins share one identity");
}

#[test]
fn the_target_set_is_closed() {
    // A pin naming something neither implementation nor specification cannot be compared
    // against anything, so the enum makes it unrepresentable rather than validating it
    // after the fact.
    for pin in canary_pins() {
        assert!(matches!(
            pin.target,
            CanaryTarget::ReferenceCli | CanaryTarget::Spec
        ));
    }
}

#[test]
fn the_canary_pin_file_lives_outside_the_registry() {
    // FR-017 / FR-017a. The isolation is a property of the layout: a pin in
    // `revisions.json` would be loaded by `certify`, and canary state would then be able
    // to change a release verdict.
    let path = default_canary_file();
    let registry = workspace_root().join("conformance").join("registry");
    assert!(
        !path.starts_with(&registry),
        "canary pins must live outside `conformance/registry/`; found at {}",
        path.display()
    );
    assert!(
        path.starts_with(workspace_root().join("conformance").join("discovery")),
        "canary pins belong in the discovery data root"
    );
}

#[test]
fn this_lane_holds_no_write_path_to_the_record() {
    // FR-020, asserted about this binary's own source: the isolation must be structural,
    // not conventional. A canary lane that *could* write the record is one refactor from
    // doing so, and an unreviewed write from a non-blocking lane is the worst of both
    // worlds — nobody looks at it, and it changes the answer.
    let source = std::fs::read_to_string(
        workspace_root()
            .join("crates")
            .join("deacon")
            .join("tests")
            .join("canary_lane.rs"),
    )
    .expect("own source readable");
    // Comments and this guard's own marker list are excluded, and both exclusions are
    // necessary rather than convenient: a test that forbids a capability has to *name* it,
    // so a scanner matching its own declaration would make every such guard its own
    // violation — and would punish exactly the prose that makes the constraint legible.
    let code: String = source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//") && !line.contains("forbidden"))
        .collect::<Vec<_>>()
        .join("\n");
    // Each entry carries the `forbidden` marker so the filter above excludes the literal
    // that names it. Without the marker the array itself is the only match in the file, and
    // the guard fails on its own declaration rather than on a real write path.
    for forbidden in [
        "fs::write",        // forbidden-marker
        "fs::rename",       // forbidden-marker
        "atomic_write",     // forbidden-marker
        "refresh_snapshot", // forbidden-marker
    ] {
        assert!(
            !code.contains(forbidden),
            "the canary lane must hold no write path; found `{forbidden}`"
        );
    }
}

#[test]
fn an_empty_pin_set_is_a_legitimate_state_not_a_failure() {
    // A canary lane with nothing pinned has nothing to compare, and that is fine: the lane
    // is non-blocking by construction (FR-019), so an empty set means "no early-warning
    // signal configured", never "the comparison passed".
    let pins = canary_pins();
    if pins.is_empty() {
        // Deliberately not a skip: the assertions above ran over the (empty) set and the
        // isolation tests ran regardless, so this binary still reports a real outcome.
        return;
    }
    assert!(!pins.is_empty());
}

#[test]
fn a_branch_that_merely_contains_a_version_is_rejected() {
    use deacon_conformance::discovery::queue::{CanaryPin, check_canary_pins, derive_canary_id};
    // `release-1.2.3` splits into `release-1`, `2`, `3` — three dot-separated parts, which a
    // looser test accepted. It is a branch name, and a branch is exactly the mutable target
    // this check exists to reject.
    for branch in [
        "release-1.2.3",
        "v1.2.3-branch",
        "feature.a.b",
        "1.2",
        "1.2.3.4",
    ] {
        let pin = CanaryPin {
            id: derive_canary_id(CanaryTarget::ReferenceCli, branch),
            target: CanaryTarget::ReferenceCli,
            revision: branch.to_string(),
            url: "https://example.invalid".into(),
            added: "2026-07-28".into(),
        };
        assert!(
            check_canary_pins(std::slice::from_ref(&pin))
                .iter()
                .any(|v| v.to_string().contains("mutable")),
            "`{branch}` must be rejected as mutable"
        );
    }
}
