//! PR-gate test (T015; research Decision 8): the REAL authoritative registry under
//! `conformance/registry/` MUST validate cleanly on every PR.
//!
//! This is the hermetic guard that keeps the registry from silently rotting. It runs
//! the full V1–V10 engine (via the shared [`validate_path`] entry) against the seed
//! skeleton (revisions/dimensions/channels/profiles today; behaviors/sources/cases
//! arrive in later phases and must keep this green). No Docker, no network, light
//! filesystem — so it lands in `dev-fast`/CI automatically with no nextest group
//! override (verify: `cargo nextest list -E 'binary(=registry_valid)'`).

use deacon_conformance::validate::validate_path;
use deacon_conformance::{default_registry_dir, workspace_root};

/// A fixed injected "today" so the gate never depends on the wall clock. The seed
/// registry has no waivers, so V6 cannot fire regardless — but pinning the date
/// keeps the test deterministic as waivers are added.
const TODAY: &str = "2026-07-19";

#[test]
fn real_registry_is_structurally_valid() {
    let registry = default_registry_dir();
    let violations = validate_path(&registry, TODAY, &workspace_root()).unwrap_or_else(|e| {
        panic!(
            "the real registry at {} is unreadable: {e}",
            registry.display()
        )
    });
    assert!(
        violations.is_empty(),
        "conformance/registry/ must validate cleanly (V1–V10 + SCHEMA); violations:\n{violations:#?}"
    );
}
