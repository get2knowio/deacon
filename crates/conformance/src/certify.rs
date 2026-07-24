//! Strict certification for the active profile (T019; FR-025;
//! 020-schema-constraint-inventory T037).
//!
//! Certification is the release gate. A registry is certified iff it is structurally
//! valid AND there is nothing blocking: no gap record exists, no in-profile behavior
//! is uncovered (data-model.md "Derived evaluations"), AND the schema-constraint
//! inventory join is clean — no V11/V12/V13/V14 violation (contracts/cli-inventory.md
//! `certify` interactions: "exit 1 iff gap OR uncovered in-profile behavior OR
//! unclassified/stale/duplicated constraint OR provenance breakage"). Waivers do NOT
//! block certification — they are enumerated in the output as characterized, harness-
//! verified divergences (research Decision 5). Neither do `not-applicable` /
//! `non-testable` classifications: a well-formed one produces NO V-class violation in
//! the first place (see [`crate::validate::check_inventory`]), so it is never a
//! blocker — it is the honest consumer-only-scope boundary, kept visible in `report`
//! (FR-014) but non-blocking here.
//!
//! This module computes the certification VERDICT from a validated registry plus the
//! committed inventory + vendored pinned schemas (the [`InventoryInputs`] the CLI
//! resolves as siblings of the registry dir); the CLI
//! (`crates/conformance/src/bin/conformance.rs`) runs the registry-only structural
//! validation (V1–V10) first, then this gate, and maps the verdict to the contract
//! exit codes (0 certified, 1 not certified / invalid, 2 usage/IO). The inventory
//! join reuses [`check_inventory`] (Phase 4), so certification and `validate` share
//! ONE join implementation — there is no parallel check. Reading the pinned schemas
//! + committed inventory is the only IO; the registry is already in memory.

use std::path::Path;

use serde::Serialize;

use crate::coverage::Coverage;
use crate::load::Registry;
use crate::model::{CaseKind, OracleType};
use crate::validate::{ClauseInputs, InventoryInputs, check_clause_inventory, check_inventory};

/// Why a certification is blocked: an unresolved gap record, an in-profile behavior
/// with no structural coverage, or a schema-constraint-inventory join violation
/// (V11–V14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockingKind {
    /// A `gap-*` record — gaps always block strict certification (FR-020, FR-025).
    Gap,
    /// An in-profile behavior with no case, waiver, or gap (would be V5-invalid; kept
    /// as an explicit blocker so certification is defensive, not merely V5-implied).
    Uncovered,
    /// A schema-constraint-inventory join violation (V11 stale classification, V12
    /// unclassified/duplicated unit, V13 malformed classification, V14 provenance
    /// breakage). The [`Blocking::code`] carries the specific class
    /// (020-schema-constraint-inventory T037; contracts/cli-inventory.md).
    Constraint,
    /// A normative-clause-inventory join violation (V11 stale, V12 unclassified/ambiguous/
    /// duplicated clause, V13 malformed classification, V14 provenance, V15 clause↔source
    /// integrity). The [`Blocking::code`] carries the specific class
    /// (021-normative-clause-inventory; contracts/clause-classification-schema.md).
    Clause,
}

/// One blocking item: its `kind`, the offending record ID, and — for a `constraint`
/// blocker only — the specific violation class `code` (`"V11"`..`"V14"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blocking {
    pub kind: BlockingKind,
    pub id: String,
    /// The violation class for a `constraint` blocker (`"V11"`..`"V14"`); absent for
    /// `gap` / `uncovered` blockers, whose kind is already fully descriptive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Committed-snapshot coverage for one snapshot-oracle case (022-conformance-runner US2;
/// NON-BLOCKING info surfaced in `certify`, FR-042).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotCoverage {
    /// The snapshot-oracle case id.
    pub case_id: String,
    /// The `os-arch` platform keys that have a committed snapshot, sorted.
    pub platforms: Vec<String>,
}

/// The certification verdict for the active profile (contracts/cli.md `certify`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Certification {
    /// True iff there are no blocking items.
    pub certified: bool,
    /// The active profile's ID (empty if the registry has no active profile).
    pub profile: String,
    /// Every blocking item, sorted by kind (gaps, then uncovered, then constraint
    /// V11–V14) then ID.
    pub blocking: Vec<Blocking>,
    /// All waiver IDs — enumerated, non-blocking (FR-025), ID-sorted.
    pub waived: Vec<String>,
    /// NON-BLOCKING info (022-conformance-runner US2, T073): committed-snapshot coverage
    /// per snapshot-oracle case — surfaced so a reviewer sees which platforms are pinned,
    /// but never a blocker (a snapshot is a reviewed artifact, not a release gate; FR-042).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshot_coverage: Vec<SnapshotCoverage>,
    /// NON-BLOCKING info: snapshot-oracle case ids with NO committed snapshot on ANY
    /// platform — a `no-reference-for-platform` coverage gap, surfaced but never blocking
    /// (FR-016a/042), ID-sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_reference: Vec<String>,
}

/// Evaluate strict certification over a VALIDATED registry plus its committed
/// inventory + vendored pinned schemas (the caller must have run structural
/// validation first; a schema-invalid or V1–V10-violating registry is "not certified"
/// at the CLI tier before this is reached).
///
/// Blocking items, in order: every `gap-*` record (FR-020/FR-025), then every
/// uncovered in-profile behavior (V5), then every schema-constraint-inventory join
/// violation (V11–V14, via [`check_inventory`] — the SAME implementation `validate`
/// runs, never a parallel copy). `not-applicable` / `non-testable` classifications do
/// NOT appear: a well-formed one produces no violation, so it can never be a blocker.
/// For a fixture registry that ships neither a committed inventory nor a vendored
/// schemas directory, [`check_inventory`] scopes itself out and contributes nothing —
/// certification then reduces to the gap/uncovered gate exactly as before this wiring.
pub fn certify(
    registry: &Registry,
    inventory: &InventoryInputs,
    clauses: &ClauseInputs,
    snapshots_dir: &Path,
) -> Certification {
    let coverage = Coverage::evaluate(registry);

    let profile = coverage.profile.map(|p| p.id.clone()).unwrap_or_default();

    // Gaps always block (FR-020, FR-025). ID-sorted for determinism.
    let mut gap_ids: Vec<&str> = registry.gaps.iter().map(|g| g.id.as_str()).collect();
    gap_ids.sort_unstable();

    // Uncovered in-profile behaviors block (V5 would already reject these, but
    // certification lists them explicitly). ID-sorted.
    let mut uncovered_ids: Vec<&str> = coverage.uncovered().iter().map(|b| b.id.as_str()).collect();
    uncovered_ids.sort_unstable();

    // Schema-constraint-inventory join violations (V11–V14) block certification
    // (contracts/cli-inventory.md). `check_inventory` already returns them sorted by
    // code then record; each blocker carries its class code so the output pinpoints
    // which of stale/unclassified/malformed/provenance failed.
    let inventory_blockers = check_inventory(registry, inventory);

    // Normative-clause-inventory join violations (V11–V15) block certification
    // (021-normative-clause-inventory; wired last per research Decision 10). The SAME
    // implementation `validate` runs.
    let clause_blockers = check_clause_inventory(registry, clauses);

    // Blocking order: all gaps first, then all uncovered, then all constraint
    // violations, then all clause violations (each group deterministically ordered).
    let mut blocking: Vec<Blocking> = Vec::with_capacity(
        gap_ids.len() + uncovered_ids.len() + inventory_blockers.len() + clause_blockers.len(),
    );
    blocking.extend(gap_ids.into_iter().map(|id| Blocking {
        kind: BlockingKind::Gap,
        id: id.to_string(),
        code: None,
    }));
    blocking.extend(uncovered_ids.into_iter().map(|id| Blocking {
        kind: BlockingKind::Uncovered,
        id: id.to_string(),
        code: None,
    }));
    blocking.extend(inventory_blockers.into_iter().map(|v| Blocking {
        kind: BlockingKind::Constraint,
        id: v.record,
        code: Some(v.code),
    }));
    blocking.extend(clause_blockers.into_iter().map(|v| Blocking {
        kind: BlockingKind::Clause,
        id: v.record,
        code: Some(v.code),
    }));

    let mut waived: Vec<String> = registry.waivers.iter().map(|w| w.id.clone()).collect();
    waived.sort();

    let (snapshot_coverage, no_reference) = snapshot_coverage(registry, snapshots_dir);

    Certification {
        // Snapshot coverage / no-reference are NON-BLOCKING info — `certified` depends
        // ONLY on `blocking` (gaps/uncovered/inventory/clause), unchanged.
        certified: blocking.is_empty(),
        profile,
        blocking,
        waived,
        snapshot_coverage,
        no_reference,
    }
}

/// Compute the NON-BLOCKING committed-snapshot coverage (T073): for every declarative
/// `snapshot`-oracle case, the `os-arch` platforms with a committed snapshot under
/// `snapshots_dir` (`<os-arch>/<case-id>/`). Cases with a snapshot go into
/// `SnapshotCoverage`; cases with none go into the `no_reference` list. A missing
/// snapshots directory yields empty coverage and every snapshot-oracle case as
/// no-reference (all deterministically ID/key-sorted).
fn snapshot_coverage(
    registry: &Registry,
    snapshots_dir: &Path,
) -> (Vec<SnapshotCoverage>, Vec<String>) {
    let mut snapshot_cases: Vec<&str> = registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .filter(|c| c.oracle_type == Some(OracleType::Snapshot))
        .map(|c| c.id.as_str())
        .collect();
    snapshot_cases.sort_unstable();

    let mut coverage = Vec::new();
    let mut no_reference = Vec::new();
    for case_id in snapshot_cases {
        let mut platforms = platforms_with_snapshot(snapshots_dir, case_id);
        platforms.sort();
        if platforms.is_empty() {
            no_reference.push(case_id.to_string());
        } else {
            coverage.push(SnapshotCoverage {
                case_id: case_id.to_string(),
                platforms,
            });
        }
    }
    (coverage, no_reference)
}

/// The `os-arch` keys under `snapshots_dir/<os-arch>/<case-id>/` that hold a committed
/// snapshot for `case_id`.
fn platforms_with_snapshot(snapshots_dir: &Path, case_id: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(snapshots_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for os_arch in entries.flatten() {
        if os_arch.path().join(case_id).is_dir() {
            out.push(os_arch.file_name().to_string_lossy().into_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn valid_registry() -> Registry {
        let root = crate::workspace_root().join("fixtures/conformance/valid");
        Registry::load(&root).expect("valid fixture loads")
    }

    /// Inventory inputs pointing at absent paths, so [`check_inventory`] scopes itself
    /// out (these fixtures ship no committed inventory / vendored schemas). The V11–V14
    /// join is exercised end-to-end in `tests/classification_join.rs` and
    /// `tests/gap_certification.rs`; these unit tests isolate the gap/uncovered gate.
    fn no_inventory() -> InventoryInputs<'static> {
        InventoryInputs {
            schemas_dir: Path::new("/nonexistent-conformance/schemas"),
            inventory_file: Path::new("/nonexistent-conformance/inventory/constraints.json"),
        }
    }

    /// Clause inputs pointing at absent paths, so [`check_clause_inventory`] scopes itself
    /// out (these fixtures ship no committed clause inventory / vendored prose).
    fn no_clauses() -> ClauseInputs<'static> {
        ClauseInputs {
            spec_dir: Path::new("/nonexistent-conformance/spec"),
            clauses_file: Path::new("/nonexistent-conformance/inventory/clauses.json"),
        }
    }

    /// A snapshots dir pointing at an absent path (these fixtures ship no snapshots); the
    /// real-registry snapshot coverage is exercised by `gap_certification.rs`.
    fn no_snapshots() -> &'static Path {
        Path::new("/nonexistent-conformance/snapshots")
    }

    #[test]
    fn valid_fixture_with_a_gap_is_not_certified() {
        // The valid fixture carries `gap-readconfig-remote-user`, so it is structurally
        // valid yet NOT certified — a gap always blocks (FR-020, FR-025).
        let registry = valid_registry();
        let result = certify(&registry, &no_inventory(), &no_clauses(), no_snapshots());
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
        let result = certify(&registry, &no_inventory(), &no_clauses(), no_snapshots());
        assert!(result.certified, "empty registry must certify");
        assert!(result.blocking.is_empty());
        assert!(result.waived.is_empty());
    }

    #[test]
    fn snapshot_coverage_is_info_only_and_never_blocks() {
        use crate::model::{OracleType, TestCase};
        // A snapshot-oracle case with a committed snapshot on one platform, and one with
        // none. Neither affects `certified` (T073, FR-042).
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("linux-x86_64/case-has-snap")).unwrap();

        let mut registry = Registry::default();
        for id in ["case-has-snap", "case-no-snap"] {
            registry.cases.push(TestCase {
                id: id.to_string(),
                oracle_type: Some(OracleType::Snapshot),
                operations: vec![crate::model::Operation {
                    id: "op".to_string(),
                    subcommand: "read-configuration".to_string(),
                    ..Default::default()
                }],
                ..TestCase::default()
            });
        }

        let result = certify(&registry, &no_inventory(), &no_clauses(), dir.path());
        assert!(
            result.certified,
            "snapshot coverage must NOT block certification"
        );
        assert_eq!(result.snapshot_coverage.len(), 1);
        assert_eq!(result.snapshot_coverage[0].case_id, "case-has-snap");
        assert_eq!(result.snapshot_coverage[0].platforms, vec!["linux-x86_64"]);
        assert_eq!(result.no_reference, vec!["case-no-snap".to_string()]);
    }
}
