//! Strict certification for the active profile (T019; FR-025).
//!
//! Certification is the release gate. A registry is certified iff it is structurally
//! valid AND there is nothing blocking: no gap record exists and no in-profile
//! behavior is uncovered (data-model.md "Derived evaluations"). Waivers do NOT block
//! certification — they are enumerated in the output as characterized, harness-
//! verified divergences (research Decision 5).
//!
//! This module computes the certification VERDICT from a validated registry; the
//! CLI (`crates/conformance/src/bin/conformance.rs`) runs validation first and maps
//! the verdict to the contract exit codes (0 certified, 1 not certified / invalid,
//! 2 usage/IO). Pure in-memory computation — no IO.

use serde::Serialize;

use crate::coverage::Coverage;
use crate::load::Registry;

/// Why a certification is blocked: an unresolved gap record, or an in-profile
/// behavior with no structural coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockingKind {
    /// A `gap-*` record — gaps always block strict certification (FR-020, FR-025).
    Gap,
    /// An in-profile behavior with no case, waiver, or gap (would be V5-invalid; kept
    /// as an explicit blocker so certification is defensive, not merely V5-implied).
    Uncovered,
}

/// One blocking item: its `kind` and the offending record ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blocking {
    pub kind: BlockingKind,
    pub id: String,
}

/// The certification verdict for the active profile (contracts/cli.md `certify`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Certification {
    /// True iff there are no blocking items.
    pub certified: bool,
    /// The active profile's ID (empty if the registry has no active profile).
    pub profile: String,
    /// Every blocking item, sorted by kind (gaps before uncovered) then ID.
    pub blocking: Vec<Blocking>,
    /// All waiver IDs — enumerated, non-blocking (FR-025), ID-sorted.
    pub waived: Vec<String>,
}

/// Evaluate strict certification over a VALIDATED registry (the caller must have run
/// validation first; a schema-invalid or violation-bearing registry is "not
/// certified" at the CLI tier before this is reached).
pub fn certify(registry: &Registry) -> Certification {
    let coverage = Coverage::evaluate(registry);

    let profile = coverage.profile.map(|p| p.id.clone()).unwrap_or_default();

    // Gaps always block (FR-020, FR-025). ID-sorted for determinism.
    let mut gap_ids: Vec<&str> = registry.gaps.iter().map(|g| g.id.as_str()).collect();
    gap_ids.sort_unstable();

    // Uncovered in-profile behaviors block (V5 would already reject these, but
    // certification lists them explicitly). ID-sorted.
    let mut uncovered_ids: Vec<&str> = coverage.uncovered().iter().map(|b| b.id.as_str()).collect();
    uncovered_ids.sort_unstable();

    // Blocking order: all gaps first, then all uncovered (each ID-sorted).
    let mut blocking: Vec<Blocking> = Vec::with_capacity(gap_ids.len() + uncovered_ids.len());
    blocking.extend(gap_ids.into_iter().map(|id| Blocking {
        kind: BlockingKind::Gap,
        id: id.to_string(),
    }));
    blocking.extend(uncovered_ids.into_iter().map(|id| Blocking {
        kind: BlockingKind::Uncovered,
        id: id.to_string(),
    }));

    let mut waived: Vec<String> = registry.waivers.iter().map(|w| w.id.clone()).collect();
    waived.sort();

    Certification {
        certified: blocking.is_empty(),
        profile,
        blocking,
        waived,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_registry() -> Registry {
        let root = crate::workspace_root().join("fixtures/conformance/valid");
        Registry::load(&root).expect("valid fixture loads")
    }

    #[test]
    fn valid_fixture_with_a_gap_is_not_certified() {
        // The valid fixture carries `gap-readconfig-remote-user`, so it is structurally
        // valid yet NOT certified — a gap always blocks (FR-020, FR-025).
        let registry = valid_registry();
        let result = certify(&registry);
        assert!(!result.certified, "a registry with a gap must not certify");
        assert!(
            result
                .blocking
                .iter()
                .any(|b| b.kind == BlockingKind::Gap && b.id == "gap-readconfig-remote-user"),
            "the gap must be listed as blocking: {:?}",
            result.blocking
        );
        // The waiver is enumerated but does NOT block.
        assert!(
            result
                .waived
                .contains(&"wvr-readconfig-malformed-jsonc".to_string())
        );
        assert_eq!(result.profile, "prof-linux-amd64-docker-0870");
    }

    #[test]
    fn empty_registry_certifies_cleanly() {
        // Nothing in-profile, no gaps → certified (mirrors the real seed registry).
        let registry = Registry::default();
        let result = certify(&registry);
        assert!(result.certified, "empty registry must certify");
        assert!(result.blocking.is_empty());
        assert!(result.waived.is_empty());
    }
}
