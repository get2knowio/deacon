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

use crate::coverage::{Coverage, ObligationBucket, evaluate_obligations};
use crate::load::Registry;
use crate::model::{CaseKind, OracleType};
use crate::obligation::{DispositionKind, generate_obligations};
use crate::validate::{
    ClauseInputs, InventoryInputs, check_clause_inventory, check_inventory,
    check_obligation_dispositions,
};

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
    /// An obligation-disposition failure (024-deterministic-conformance-coverage, US2;
    /// contracts/obligation.md "Certification integration"). The [`Blocking::code`]
    /// carries the specific class — the same shape `Constraint` and `Clause` already use,
    /// so the output format does not fork into a third:
    ///
    /// | `code` | Condition |
    /// |---|---|
    /// | `V28` | an applicable obligation with zero dispositions, or with more than one |
    /// | `V29` | a malformed or stale disposition |
    /// | `V6` | a `waived` disposition whose waiver has expired |
    ///
    /// A `gap` disposition is deliberately absent: it blocks through the `gap-` record it
    /// names, which V29 already requires to resolve and which [`BlockingKind::Gap`]
    /// already reports. Listing it twice would double-count one fact ("existing gap
    /// semantics", contracts/obligation.md).
    Obligation,
}

/// One blocking item: its `kind`, the offending record ID, and — for a `constraint`
/// blocker only — the specific violation class `code` (`"V11"`..`"V14"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blocking {
    pub kind: BlockingKind,
    pub id: String,
    /// The violation class for a `constraint` / `clause` / `obligation` blocker
    /// (`"V6"`, `"V11"`..`"V15"`, `"V28"`, `"V29"`); absent for `gap` / `uncovered`
    /// blockers, whose kind is already fully descriptive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// The five FR-026 coverage buckets over the generated obligation set, plus the
/// undispositioned count — **never folded together** (024-deterministic-conformance-
/// coverage, T068).
///
/// Reported ALONGSIDE the behavior-level numbers `certify` already carries, not instead
/// of them: the two denominators answer different questions (which behaviors are
/// evidenced, versus which modelled combinations are exercised), and collapsing them
/// would let progress on one hide the absence of progress on the other.
///
/// `undispositioned` is not a sixth bucket in the FR-026 sense — it is the queue SC-001
/// requires to reach zero, and every entry in it is simultaneously a `V28` blocker above.
/// It is counted separately because folding it into `gap` would overstate what is known
/// and folding it into `covered` would understate the hole.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationSummary {
    /// Every generated obligation — the denominator.
    pub total: usize,
    /// Dispositioned `case`, or (absent a record) backed by matching evidence.
    pub covered: usize,
    /// Dispositioned `waived`. Non-blocking until the waiver expires.
    pub waived: usize,
    /// Dispositioned `non-testable`. Non-blocking.
    pub non_testable: usize,
    /// Dispositioned `gap`. Blocks through the `gap-` record it names.
    pub gap: usize,
    /// Modelled, but its environment is not the active profile: enumerated as visible
    /// backlog, counted as neither covered nor gap, never blocking (spec Assumption 11).
    pub inactive_environment: usize,
    /// Carrying no explicit disposition — each one also a `V28` blocker.
    pub undispositioned: usize,
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

/// One residual record surfaced as NON-BLOCKING certification information
/// (023-migrate-parity-to-conformance, FR-054).
///
/// A residual is *representation debt*, not a coverage gap: the behavior is still
/// covered — by the carrier program that has not been retired yet. It is therefore
/// listed so a reviewer sees the queue and what it blocks, but it NEVER contributes to
/// `blocking`. Only a `gap-` record admits missing coverage, and only a gap blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidualInfo {
    /// The `res-` record id.
    pub id: String,
    /// The program that cannot be deleted while this residual stands (absent for the
    /// `external-corpus-entry` residuals, which block no program — research D8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_carrier: Option<String>,
    /// The specific named capability the declarative system lacks.
    pub missing_capability: String,
    /// The tracked follow-up reference — present iff this is a QUEUED residual (024 P1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    /// Why the unit is permanently inexpressible — present iff PERMANENT (024 P1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_scope_rationale: Option<String>,
    /// How many baseline units this residual covers.
    pub units: usize,
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
    /// NON-BLOCKING info (023-migrate-parity-to-conformance, T035, FR-054): the residual
    /// queue — representation debt that is still MIGRATABLE, ID-sorted. Listed so the queue
    /// is visible and its blocked carriers are known; NEVER a certification blocker (only
    /// gaps admit missing coverage).
    ///
    /// Permanent exclusions are deliberately NOT here (024 P1): mixing them in would make
    /// the queue asymptote at a nonzero floor forever, and a number that can never reach
    /// zero cannot be read as progress. See [`permanent_residuals`](Self::permanent_residuals).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_queue: Vec<ResidualInfo>,
    /// NON-BLOCKING info (024 P1): residuals that can NEVER be expressed as data, each
    /// carrying the principle or category mismatch that forbids it, ID-sorted. Separated
    /// from `residual_queue` so "the queue reaches zero" stays a meaningful claim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permanent_residuals: Vec<ResidualInfo>,
    /// NON-BLOCKING info (023 T061): normalization rules registered with a DECLARED
    /// FR-021 deficiency — `(rule name, reason)`, name-sorted. Surfaced for the same
    /// reason as the residual queue: an admitted, tracked deficiency is debt, not a
    /// missing-coverage claim. An UNDECLARED blanket rule is V24 and blocks a PR.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_compliant_rules: Vec<NonCompliantRule>,
    /// The five FR-026 obligation buckets plus the undispositioned queue (024 US2,
    /// T068). Informational; what blocks is in `blocking`.
    #[serde(default)]
    pub obligations: ObligationSummary,
}

/// A normalization rule registered with a declared FR-021 deficiency (T061).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NonCompliantRule {
    /// The rule's name.
    pub name: String,
    /// Why it does not satisfy FR-021, and where the narrowing work is tracked.
    pub reason: String,
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
    today: &str,
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

    // Obligation-disposition violations (V28/V29) block certification (024 US2,
    // contracts/obligation.md). The SAME implementation `validate` runs — an obligation
    // gate that disagreed with `validate` about what is undispositioned would make the
    // release verdict depend on which command you happened to run.
    let disposition_blockers = check_obligation_dispositions(registry);
    // Plus the one condition `check_obligation_dispositions` cannot see, because it is
    // date-dependent: a `waived` disposition whose waiver has expired (V6). The waiver
    // itself is what expired, so the waiver is what the blocker names — the record V6
    // names everywhere else in the codebase.
    let expired = expired_waiver_dispositions(registry, today);

    // Blocking order: all gaps first, then all uncovered, then all constraint
    // violations, then all clause violations, then all obligation violations (each group
    // deterministically ordered).
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
    blocking.extend(disposition_blockers.into_iter().map(|v| Blocking {
        kind: BlockingKind::Obligation,
        id: v.record,
        code: Some(v.code),
    }));
    blocking.extend(expired.into_iter().map(|id| Blocking {
        kind: BlockingKind::Obligation,
        id,
        code: Some("V6".to_string()),
    }));

    let mut waived: Vec<String> = registry.waivers.iter().map(|w| w.id.clone()).collect();
    waived.sort();

    let (snapshot_coverage, no_reference) = snapshot_coverage(registry, snapshots_dir);
    let (residual_queue, permanent_residuals) = residuals_by_disposition(registry);
    let non_compliant_rules =
        crate::conservation::declared_non_compliant_rules(crate::conservation::NORMALIZATION_RULES)
            .into_iter()
            .map(|(name, reason)| NonCompliantRule {
                name: name.to_string(),
                reason: reason.to_string(),
            })
            .collect();

    Certification {
        // Snapshot coverage / no-reference / the residual queue are NON-BLOCKING info —
        // `certified` depends ONLY on `blocking` (gaps/uncovered/inventory/clause),
        // unchanged.
        certified: blocking.is_empty(),
        profile,
        blocking,
        waived,
        snapshot_coverage,
        no_reference,
        residual_queue,
        permanent_residuals,
        non_compliant_rules,
        obligations: obligation_summary(registry),
    }
}

/// The waiver ids named by a `waived` disposition whose waiver has already expired,
/// id-sorted and de-duplicated (T067; SC-009).
///
/// V6 already reports an expired waiver during `validate`, but `certify` has never
/// blocked on one — a waiver is a decision that no further work is needed, and its
/// expiry is a prompt to re-confirm, not evidence that something broke. An obligation
/// dispositioned `waived` is different: the waiver is the ONLY thing standing between
/// that obligation and "undispositioned". Once it expires, nothing does.
///
/// The boundary passes (`expires == today` is still valid), matching V6 exactly, so the
/// two cannot disagree by a day about when a waiver dies.
fn expired_waiver_dispositions(registry: &Registry, today: &str) -> Vec<String> {
    let mut out: Vec<String> = registry
        .obligation_dispositions
        .iter()
        .filter(|record| record.disposition == DispositionKind::Waived)
        .filter_map(|record| record.waiver.as_deref())
        .filter(|id| {
            registry
                .waivers
                .iter()
                .any(|w| w.id == *id && w.expires.as_str() < today)
        })
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Count the five FR-026 buckets plus the undispositioned queue over the generated
/// obligation set (T068).
///
/// Regenerated rather than read from `conformance/obligations/obligations.json`: V27
/// already guarantees the commit byte-matches a regeneration, so the two are the same
/// numbers, and regenerating keeps this gate working for a fixture registry that ships no
/// committed inventory. A registry with no scenario model has no obligation regime and
/// reports zeros — not because it is covered, but because it declared nothing to cover.
fn obligation_summary(registry: &Registry) -> ObligationSummary {
    let Ok(inventory) = generate_obligations(registry) else {
        return ObligationSummary::default();
    };
    let mut summary = ObligationSummary {
        total: inventory.units.len(),
        ..ObligationSummary::default()
    };
    for outcome in evaluate_obligations(registry, &inventory) {
        let bucket = match outcome.bucket {
            ObligationBucket::Covered => &mut summary.covered,
            ObligationBucket::Waived => &mut summary.waived,
            ObligationBucket::NonTestable => &mut summary.non_testable,
            ObligationBucket::Gap => &mut summary.gap,
            ObligationBucket::InactiveEnvironment => &mut summary.inactive_environment,
            ObligationBucket::Undispositioned => &mut summary.undispositioned,
        };
        *bucket += 1;
    }
    summary
}

/// Partition the NON-BLOCKING residuals into `(queued, permanent)` (T035, FR-054; 024 P1),
/// each ID-sorted. Reported alongside the blockers so a reviewer sees the representation
/// debt and which carriers it pins, but never folded into `blocking`: residuals are debt,
/// gaps are missing coverage, and only the latter can block a release.
///
/// The partition is the point. Queued residuals are work, and their count is meant to fall
/// to zero; permanent ones never will, so counting them together would produce a number
/// that looks like a stalled queue forever.
fn residuals_by_disposition(registry: &Registry) -> (Vec<ResidualInfo>, Vec<ResidualInfo>) {
    let (mut permanent, mut queued): (Vec<ResidualInfo>, Vec<ResidualInfo>) = registry
        .residuals
        .iter()
        .map(|r| {
            (
                r.disposition.is_permanent(),
                ResidualInfo {
                    id: r.id.clone(),
                    blocked_carrier: r.blocked_carrier.clone(),
                    missing_capability: r.missing_capability.clone(),
                    follow_up: r.follow_up.clone(),
                    out_of_scope_rationale: r.out_of_scope_rationale.clone(),
                    units: r.units.len(),
                },
            )
        })
        .fold(
            (Vec::new(), Vec::new()),
            |(mut perm, mut queue), (is_permanent, info)| {
                if is_permanent {
                    perm.push(info);
                } else {
                    queue.push(info);
                }
                (perm, queue)
            },
        );
    queued.sort_by(|a, b| a.id.cmp(&b.id));
    permanent.sort_by(|a, b| a.id.cmp(&b.id));
    (queued, permanent)
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

    /// A fixed injected "today" so the V6 expired-waiver-disposition gate never depends
    /// on the wall clock (these fixtures' waivers expire in 2027).
    const TODAY: &str = "2026-07-19";

    #[test]
    fn valid_fixture_with_a_gap_is_not_certified() {
        // The valid fixture carries `gap-readconfig-remote-user`, so it is structurally
        // valid yet NOT certified — a gap always blocks (FR-020, FR-025).
        let registry = valid_registry();
        let result = certify(
            &registry,
            TODAY,
            &no_inventory(),
            &no_clauses(),
            no_snapshots(),
        );
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
        let result = certify(
            &registry,
            TODAY,
            &no_inventory(),
            &no_clauses(),
            no_snapshots(),
        );
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

        let result = certify(&registry, TODAY, &no_inventory(), &no_clauses(), dir.path());
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
