//! Structural validation engine (violation classes V1–V20 + SCHEMA), FR-019.
//!
//! [`run`] evaluates the registry-only violation classes (V1–V10, plus **V16**
//! declarative-case well-formedness, **V18** Docker-case pinned-input enforcement, **V19**
//! allowed-difference identity resolution, and **V20** invariant-metamorphic arity —
//! 022-conformance-runner) over a loaded [`Registry`] and returns ALL violations found in
//! a single pass (never first-failure),
//! sorted by code then record ID (contracts/cli.md). **V17** (committed-snapshot
//! integrity) runs in [`validate_path_with_inventory`], scoped to the snapshots sibling
//! of the registry.
//! [`check_inventory`] adds the schema-constraint-inventory join classes (V11–V14),
//! which need the committed inventory + vendored schemas alongside the registry
//! (020-schema-constraint-inventory). [`validate_path`] is the registry-only
//! load-then-validate convenience the V1–V10 acceptance tests and the `report` /
//! `certify` gates share; [`validate_path_with_inventory`] is the superset the
//! `validate` CLI command runs (V1–V14 together in one pass). A schema-invalid
//! registry folds its located [`SchemaError`]s into `SCHEMA`-class violations; a
//! genuinely unreadable registry root is the only outcome surfaced as an `Err` (CLI
//! exit 2).
//!
//! The violation classes (data-model.md):
//!
//! - **V1** dangling references (record IDs, dimension values, orphan behaviors) and
//!   missing executable test binaries (research Decision 9);
//! - **V2** duplicate stable IDs anywhere in the registry, ID-format violations, and
//!   prefix↔type mismatches (FR-004);
//! - **V3** a test case linked to no behavior;
//! - **V4** a source unit with empty `behaviors` and no `outOfScope`;
//! - **V5** an in-active-profile behavior with no case, no waiver, AND no gap;
//! - **V6** a waiver whose `expires` is earlier than today;
//! - **V7** a source revision whose `pin` disagrees with its `verifiedAgainst` file;
//! - **V8** disposition contradictions (rules R1–R8, research Decision 5), incl.
//!   extension↔decision consistency;
//! - **V9** an expected outcome referencing an undeclared observable channel;
//! - **V10** a test case whose context has an empty intersection with a linked
//!   behavior's applicability;
//! - **V11** a classification whose `constraint` is absent from the committed
//!   inventory (stale);
//! - **V12** a constraint unit with zero classification records (unclassified) or
//!   more than one (duplicated) — every unit of every kind requires exactly one;
//! - **V13** a classification whose shape/linkage is broken: id-tail mirror, the
//!   `behaviors` arity/existence rule vs `disposition`, or a missing `rationale`
//!   on a `non-testable` / `not-applicable` record;
//! - **V14** provenance breakage: schemas manifest fingerprint mismatch, an
//!   inventory `revision` that does not name the registry's `schema`-kind revision,
//!   or a committed inventory that no longer byte-matches a fresh regeneration.
//!
//! Pure sync file IO only (V1 executable existence, V7 pin file); no Unix-only APIs
//! and no path-string parsing, so the crate compiles and validates identically on
//! the Windows `dev-fast` lane.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::clause::{generate_clauses, render as render_clauses};
use crate::inventory::{generate_inventory, render};
use crate::load::{
    LoadError, Registry, SchemaError, load_clause_inventory, load_inventory, load_spec_manifest,
};
use crate::model::{
    BehaviorUnit, CONSUMER_SUBCOMMANDS, CaseKind, CertificationProfile, Classification,
    ClauseClassification, ClauseInventory, ClauseUnit, Condition, ConstraintInventory,
    ConstraintUnit, Decision, Disposition, DocumentScope, FILESYSTEM_CHANNELS, OracleType,
    RecordType, ReferenceStatus, RevisionKind, SpecManifest, SpecStatus, Strength, Testability,
    parse_id,
};
use crate::prose::Document;
use crate::prose::strength::{has_family, hides_mandatory_keyword};

/// A single structural violation. `code` is the stable class (`"V1"`..`"V10"` or
/// `"SCHEMA"`); `record` names the offending registry record (or, for SCHEMA, the
/// file); `message` is a precise, human-readable diagnosis (constitution IV).
///
/// `Serialize` produces exactly the contracts/cli.md JSON shape
/// (`{ "code", "record", "message" }`) — field names are already the wire names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub code: String,
    pub record: String,
    pub message: String,
}

impl Violation {
    fn new(code: &str, record: impl Into<String>, message: impl Into<String>) -> Violation {
        Violation {
            code: code.to_string(),
            record: record.into(),
            message: message.into(),
        }
    }
}

/// Sort rank for a violation code: `SCHEMA` first, then `V1`..`V10` numerically
/// (so `V10` sorts after `V2`, not lexicographically before it). Unknown codes sort
/// last, deterministically.
fn code_rank(code: &str) -> u32 {
    match code {
        "SCHEMA" => 0,
        other => other
            .strip_prefix('V')
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(u32::MAX),
    }
}

/// Load the registry at `root` and validate it, folding schema-load failures into
/// `SCHEMA`-class violations.
///
/// - `Ok(violations)` — the registry loaded (empty = valid) OR the load hit a
///   schema error (returned as `SCHEMA` violations, one per bad file). Both map to
///   CLI exit 0 (empty) / 1 (non-empty).
/// - `Err(LoadError::Root)` — the registry root is unreadable (CLI exit 2).
///
/// `repo_root` anchors repo-relative checks (V7 `verifiedAgainst` files, V1
/// executable-test files); it is the workspace root, NOT the registry dir, so a
/// fixture registry still resolves the real `fixtures/parity-corpus/oracle.json` and
/// `crates/*/tests/` tree.
pub fn validate_path(
    root: &Path,
    today: &str,
    repo_root: &Path,
) -> Result<Vec<Violation>, LoadError> {
    match Registry::load(root) {
        Ok(registry) => Ok(run(&registry, today, repo_root)),
        Err(LoadError::Schema(errors)) => Ok(schema_violations(&errors)),
        // `Root` and the inventory-domain errors (never produced by
        // `Registry::load`, only by the schema/inventory loaders) surface as an Err
        // — the CLI maps them to exit 2. Only `Schema` folds into SCHEMA violations.
        Err(other) => Err(other),
    }
}

/// Where to find the schema-constraint-inventory provenance inputs for the V11–V14
/// join (020-schema-constraint-inventory). The committed inventory is the join target
/// for V11/V12/V13; the pinned schemas directory drives the V14 regeneration and
/// fingerprint verification. Tests point both at fixtures; the `validate` CLI command
/// uses the workspace defaults.
#[derive(Debug, Clone, Copy)]
pub struct InventoryInputs<'a> {
    /// The pinned schemas directory (`manifest.json` + the vendored schema files).
    pub schemas_dir: &'a Path,
    /// The committed constraint inventory file.
    pub inventory_file: &'a Path,
}

/// Where to find the normative-clause-inventory provenance inputs for the V11–V15 clause
/// join (021-normative-clause-inventory). The committed clause inventory is the join
/// target; the pinned spec directory drives the V14 regeneration/fingerprint verification
/// and the V15 excerpt-present-at-anchor checks. Tests point both at fixtures.
#[derive(Debug, Clone, Copy)]
pub struct ClauseInputs<'a> {
    /// The pinned spec directory (`manifest.json` + the vendored Markdown files).
    pub spec_dir: &'a Path,
    /// The committed clause inventory file.
    pub clauses_file: &'a Path,
}

/// Load the registry at `root`, validate it (V1–V10 via [`run`]), AND enforce the
/// schema-constraint-inventory join classes (V11–V14 via [`check_inventory`]) in the
/// SAME single pass, returning ALL violations sorted by code then record ID.
///
/// This is the entry point the `validate` CLI command runs. The registry-only
/// [`validate_path`] is retained for the `report` / `certify` gates and the V1–V10
/// acceptance fixtures, which must NOT see V11–V14 (the inventory join is scoped to the
/// real inventory + vendored schemas, not per-fixture). Schema-load failures fold into
/// `SCHEMA`-class violations exactly as [`validate_path`] does; the inventory join is
/// then skipped (there is no cleanly-loaded registry to join against).
pub fn validate_path_with_inventory(
    root: &Path,
    today: &str,
    repo_root: &Path,
    inputs: &InventoryInputs,
    clause_inputs: &ClauseInputs,
) -> Result<Vec<Violation>, LoadError> {
    match Registry::load(root) {
        Ok(registry) => {
            let mut violations = run(&registry, today, repo_root);
            violations.extend(check_inventory(&registry, inputs));
            violations.extend(check_clause_inventory(&registry, clause_inputs));
            // V17: committed snapshots are a sibling of the registry dir (mirrors the
            // inventory/clause sibling resolution) — absent for fixture registries.
            let snapshots_dir = root
                .parent()
                .map(|p| p.join("snapshots"))
                .unwrap_or_else(|| root.join("snapshots"));
            violations.extend(check_snapshots(&registry, &snapshots_dir));
            sort_violations(&mut violations);
            Ok(violations)
        }
        Err(LoadError::Schema(errors)) => Ok(schema_violations(&errors)),
        Err(other) => Err(other),
    }
}

/// **V17 — committed-snapshot provenance integrity** (022-conformance-runner, US2).
/// Scans `snapshots_dir` (`<os-arch>/<case-id>/`) and flags: a snapshot whose `case-id`
/// is not a declarative case in the registry (an orphan snapshot for a
/// deleted/renamed/legacy case), and a snapshot whose `provenance.json` is missing or
/// malformed. A missing snapshots directory yields no violations (fixture registries
/// ship none). Staleness of a well-formed snapshot is NOT a validate concern — it is the
/// `snapshot check` gate (a snapshot may be legitimately stale pending a reviewed
/// refresh).
pub fn check_snapshots(registry: &Registry, snapshots_dir: &Path) -> Vec<Violation> {
    let mut out = Vec::new();
    if !snapshots_dir.is_dir() {
        return out;
    }
    let declarative: HashSet<&str> = registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .map(|c| c.id.as_str())
        .collect();

    // <snapshots>/<os-arch>/<case-id>/
    let os_arch_dirs = match std::fs::read_dir(snapshots_dir) {
        Ok(rd) => rd,
        Err(e) => {
            out.push(Violation::new(
                "V17",
                snapshots_dir.display().to_string(),
                format!("cannot read snapshots directory: {e}"),
            ));
            return out;
        }
    };
    let mut entries: Vec<(String, std::path::PathBuf)> = Vec::new();
    for os_arch in os_arch_dirs.flatten() {
        if !os_arch.path().is_dir() {
            continue;
        }
        let os_arch_name = os_arch.file_name().to_string_lossy().into_owned();
        if let Ok(cases) = std::fs::read_dir(os_arch.path()) {
            for case_dir in cases.flatten() {
                if case_dir.path().is_dir() {
                    let case_id = case_dir.file_name().to_string_lossy().into_owned();
                    entries.push((format!("{os_arch_name}/{case_id}"), case_dir.path()));
                }
            }
        }
    }
    // Deterministic order.
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, dir) in entries {
        let case_id = key.rsplit('/').next().unwrap_or(&key);
        if !declarative.contains(case_id) {
            out.push(Violation::new(
                "V17",
                key.clone(),
                format!(
                    "committed snapshot for {case_id:?} has no matching declarative case in the \
                     registry (orphan snapshot — delete it or restore the case)"
                ),
            ));
        }
        if let Err(e) = crate::snapshot::load_provenance(&dir) {
            out.push(Violation::new(
                "V17",
                key,
                format!("committed snapshot provenance is unreadable/malformed: {e}"),
            ));
        }
    }
    out
}

/// The join of the committed inventory against the hand-authored classification
/// records: the raw material behind V11 (stale) and V12 (unclassified / duplicated).
///
/// Extracted so `validate`'s enforcement and `report`'s review queues are computed by
/// ONE implementation — the two must never disagree about what is unclassified or stale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryJoin {
    /// Unit IDs carrying zero classification records (V12), ID-sorted.
    pub unclassified: Vec<String>,
    /// Classification IDs whose constraint is absent from the inventory (V11), ID-sorted.
    pub stale: Vec<String>,
    /// Units classified more than once (V12): `(unit id, the offending classification
    /// ids)`, both levels ID-sorted.
    pub duplicated: Vec<(String, Vec<String>)>,
}

/// Join `units` against `classifications` (see [`InventoryJoin`]). Pure and total — it
/// never reads the filesystem, so both callers can share it.
pub fn join_inventory(
    units: &[ConstraintUnit],
    classifications: &[Classification],
) -> InventoryJoin {
    let mut per_constraint: HashMap<&str, Vec<&str>> = HashMap::new();
    for cls in classifications {
        per_constraint
            .entry(cls.constraint.as_str())
            .or_default()
            .push(cls.id.as_str());
    }
    let unit_ids: HashSet<&str> = units.iter().map(|u| u.id.as_str()).collect();

    let mut join = InventoryJoin::default();
    for unit in units {
        match per_constraint.get(unit.id.as_str()) {
            None => join.unclassified.push(unit.id.clone()),
            Some(ids) if ids.len() > 1 => {
                let mut ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
                ids.sort();
                join.duplicated.push((unit.id.clone(), ids));
            }
            Some(_) => {}
        }
    }
    for cls in classifications {
        if !unit_ids.contains(cls.constraint.as_str()) {
            join.stale.push(cls.id.clone());
        }
    }

    join.unclassified.sort();
    join.stale.sort();
    join.duplicated.sort();
    join
}

/// Enforce the schema-constraint-inventory join classes (V11–V14) by joining the
/// loaded registry's classification records against the committed inventory and the
/// vendored pinned schemas (contracts/classification-schema.md,
/// 020-schema-constraint-inventory). Returns every violation found (sorted); an empty
/// vector means the join is clean.
///
/// - **V11 (stale)**: a classification's `constraint` is absent from the committed
///   inventory.
/// - **V12 (unclassified / duplicated)**: a constraint unit with zero classification
///   records, or with more than one. Every unit of every kind (including `annotation`
///   and `unmodeled-keyword`) requires exactly one.
/// - **V13 (shape / linkage)**: id-tail mirror, `behaviors` arity + existence vs
///   `disposition`, and `rationale` presence for `non-testable` / `not-applicable`.
/// - **V14 (provenance)**: manifest fingerprint mismatch, an inventory `revision` that
///   does not name the registry's `schema`-kind revision, or a committed inventory that
///   no longer byte-matches a fresh regeneration (reusing the `inventory check`
///   comparison via [`generate_inventory`] + [`render`]).
///
/// V11/V12 are derived from the shared [`join_inventory`], so `validate` and `report`
/// can never disagree about the review queues.
pub fn check_inventory(registry: &Registry, inputs: &InventoryInputs) -> Vec<Violation> {
    let mut out: Vec<Violation> = Vec::new();

    // A registry with NEITHER a committed inventory nor a vendored schemas directory is
    // not subject to the inventory contract (e.g. the V1–V10 acceptance fixtures, which
    // ship no inventory). The join has nothing to check. This is scoping, not a silent
    // fallback: wherever an inventory OR its schemas exist — as they always do for the
    // real `conformance/` tree — the corresponding V11–V14 checks run in full (a deleted
    // inventory with schemas still present, or vice-versa, still trips V14).
    if !inputs.inventory_file.exists() && !inputs.schemas_dir.exists() {
        return out;
    }

    // The committed inventory is the join target for V11/V12/V13. A malformed or
    // unreadable committed file is itself provenance breakage (V14): the join cannot
    // proceed, so units stay empty (every classification then reads as stale — V11 —
    // which is the correct "the machine-owned artifact is broken" signal).
    let committed: Option<ConstraintInventory> = match load_inventory(inputs.inventory_file) {
        Ok(inv) => inv,
        Err(e) => {
            out.push(Violation::new(
                "V14",
                inputs.inventory_file.display().to_string(),
                format!("could not load the committed inventory for the classification join: {e}"),
            ));
            None
        }
    };

    let units: &[ConstraintUnit] = committed
        .as_ref()
        .map(|c| c.units.as_slice())
        .unwrap_or(&[]);
    let behavior_ids: HashSet<&str> = registry.behaviors.iter().map(|b| b.id.as_str()).collect();
    let join = join_inventory(units, &registry.classifications);

    // V11 (stale), from the shared join.
    let stale: HashSet<&str> = join.stale.iter().map(String::as_str).collect();
    for cls in &registry.classifications {
        if stale.contains(cls.id.as_str()) {
            out.push(Violation::new(
                "V11",
                &cls.id,
                format!(
                    "classification references constraint {:?}, which is absent from the \
                     committed inventory (stale — delete or re-point it)",
                    cls.constraint
                ),
            ));
        }
        // V13 (shape/linkage), per classification record.
        check_classification_shape(cls, &behavior_ids, &mut out);
    }

    // V12 (unclassified / duplicated), per constraint unit — no unit is exempt.
    for id in &join.unclassified {
        out.push(Violation::new(
            "V12",
            id,
            "constraint unit has no classification record (unclassified — exactly one is \
             required)",
        ));
    }
    for (id, cls_ids) in &join.duplicated {
        out.push(Violation::new(
            "V12",
            id,
            format!(
                "constraint unit is classified by {} records ({}); exactly one is required \
                 (duplicated)",
                cls_ids.len(),
                cls_ids.join(", ")
            ),
        ));
    }

    // V14 (provenance).
    check_provenance(registry, inputs, committed.as_ref(), &mut out);

    sort_violations(&mut out);
    out
}

/// V13 shape/linkage checks for a single classification record
/// (contracts/classification-schema.md "Record rules").
fn check_classification_shape(
    cls: &Classification,
    behavior_ids: &HashSet<&str>,
    out: &mut Vec<Violation>,
) {
    // Rule 1: `id` = `cls-` + the exact tail of the `constraint`'s `cst-` id.
    match cls.constraint.strip_prefix("cst-") {
        Some(tail) => {
            let expected = format!("cls-{tail}");
            if cls.id != expected {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!(
                        "classification id must mirror its constraint tail (expected {expected:?} \
                         for constraint {:?})",
                        cls.constraint
                    ),
                ));
            }
        }
        None => out.push(Violation::new(
            "V13",
            &cls.id,
            format!(
                "classification constraint {:?} is not a `cst-` id; the id-tail mirror is undefined",
                cls.constraint
            ),
        )),
    }

    // Rules 2/3: `behaviors` arity + existence, keyed to the disposition.
    match cls.disposition {
        Disposition::BehaviorMapped => {
            if cls.behaviors.is_empty() {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    "disposition `behavior-mapped` requires a non-empty `behaviors` list",
                ));
            }
            for behavior in &cls.behaviors {
                if !behavior_ids.contains(behavior.as_str()) {
                    out.push(Violation::new(
                        "V13",
                        &cls.id,
                        format!(
                            "maps to behavior {behavior:?}, which is not an existing `bhv-` record"
                        ),
                    ));
                }
            }
        }
        Disposition::NonTestable | Disposition::NotApplicable => {
            if !cls.behaviors.is_empty() {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!(
                        "disposition {} must have an empty `behaviors` list",
                        disposition_name(cls.disposition)
                    ),
                ));
            }
        }
    }

    // Rule 4: `non-testable` / `not-applicable` require a non-empty `rationale`.
    if matches!(
        cls.disposition,
        Disposition::NonTestable | Disposition::NotApplicable
    ) {
        let has_rationale = cls
            .rationale
            .as_deref()
            .is_some_and(|r| !r.trim().is_empty());
        if !has_rationale {
            out.push(Violation::new(
                "V13",
                &cls.id,
                format!(
                    "disposition {} requires a non-empty `rationale`",
                    disposition_name(cls.disposition)
                ),
            ));
        }
    }
}

/// V14 provenance checks: regeneration byte-equality + manifest fingerprint (both via
/// [`generate_inventory`], reusing the `inventory check` comparison) and the inventory
/// revision ↔ registry `schema`-kind revision pin.
fn check_provenance(
    registry: &Registry,
    inputs: &InventoryInputs,
    committed: Option<&ConstraintInventory>,
    out: &mut Vec<Violation>,
) {
    // Fingerprint (surfaced by `generate_inventory`) + committed-vs-regenerated bytes.
    match generate_inventory(inputs.schemas_dir) {
        Ok(regenerated) => match std::fs::read_to_string(inputs.inventory_file) {
            Ok(committed_raw) => {
                if committed_raw != render(&regenerated) {
                    out.push(Violation::new(
                        "V14",
                        inputs.inventory_file.display().to_string(),
                        "committed inventory does not byte-match a fresh regeneration from the \
                         pinned schemas (run `inventory generate`)",
                    ));
                }
            }
            Err(e) => out.push(Violation::new(
                "V14",
                inputs.inventory_file.display().to_string(),
                format!(
                    "could not read the committed inventory for the regeneration comparison: {e}"
                ),
            )),
        },
        Err(LoadError::ManifestFingerprintMismatch {
            file,
            expected,
            actual,
        }) => out.push(Violation::new(
            "V14",
            file.display().to_string(),
            format!(
                "schemas manifest fingerprint mismatch: manifest records {expected}, vendored file \
                 is {actual}"
            ),
        )),
        Err(e) => out.push(Violation::new(
            "V14",
            inputs.schemas_dir.display().to_string(),
            format!(
                "could not regenerate the inventory from the pinned schemas for provenance: {e}"
            ),
        )),
    }

    // The inventory `revision` must NAME AN EXISTING `schema`-kind revision record
    // (data-model.md §1). Matching against "the first such record" would spuriously fire
    // during a pin bump, when the registry legitimately carries both the outgoing and the
    // incoming `rev-schema-*` records.
    if let Some(committed) = committed {
        let schema_revisions: Vec<&str> = registry
            .revisions
            .iter()
            .filter(|r| r.kind == RevisionKind::Schema)
            .map(|r| r.id.as_str())
            .collect();
        if schema_revisions.is_empty() {
            out.push(Violation::new(
                "V14",
                committed.revision.clone(),
                "the registry declares no `schema`-kind revision to pin the inventory against",
            ));
        } else if !schema_revisions.contains(&committed.revision.as_str()) {
            out.push(Violation::new(
                "V14",
                committed.revision.clone(),
                format!(
                    "inventory revision {:?} names no `schema`-kind revision record in the \
                     registry (declared: {})",
                    committed.revision,
                    schema_revisions.join(", ")
                ),
            ));
        }
    }
}

// ===========================================================================
// Normative clause inventory join (V11–V15) — 021-normative-clause-inventory
// ===========================================================================

/// The join of the committed clause inventory against the hand-authored
/// clause-classification records, with the document-scope resolution rule (research
/// Decision 7). The clause analogue of [`InventoryJoin`] — the same conceptual V11/V12
/// classes generalized from a constraint unit to a clause unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClauseJoin {
    /// Clause IDs with no effective disposition (V12), ID-sorted. Includes an unresolved
    /// `ambiguous` clause (no document-scope cover permitted) and a clause whose only
    /// cover would be an invalid document-scope default.
    pub unclassified: Vec<String>,
    /// Per-clause classification IDs whose `clause` is absent from the inventory (V11),
    /// ID-sorted.
    pub stale: Vec<String>,
    /// Clauses classified more than once by per-clause records (V12): `(clause id,
    /// offending classification ids)`, both ID-sorted.
    pub duplicated: Vec<(String, Vec<String>)>,
}

/// Join `units` against `classifications` with the document-scope resolution rule. Pure
/// and total (no filesystem access): a per-clause record wins; else, if the clause's
/// document is `authoring`-scope AND its testability ≠ `ambiguous`, a document-scope
/// default applies; else the clause is unclassified. `authoring_docs` is the set of
/// document keys the manifest marks `authoring`; `covered_docs` is the set of document
/// keys carrying a (valid) document-scope default record.
pub fn join_clauses(
    units: &[ClauseUnit],
    classifications: &[ClauseClassification],
    authoring_docs: &HashSet<String>,
    covered_docs: &HashSet<String>,
) -> ClauseJoin {
    // Per-clause records, keyed by clause id.
    let mut per_clause: HashMap<&str, Vec<&str>> = HashMap::new();
    for cls in classifications {
        if let Some(clause) = cls.clause.as_deref() {
            per_clause.entry(clause).or_default().push(cls.id.as_str());
        }
    }
    let unit_ids: HashSet<&str> = units.iter().map(|u| u.id.as_str()).collect();

    let mut join = ClauseJoin::default();
    for unit in units {
        match per_clause.get(unit.id.as_str()) {
            Some(ids) if ids.len() > 1 => {
                let mut ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
                ids.sort();
                join.duplicated.push((unit.id.clone(), ids));
            }
            Some(_) => {} // exactly one per-clause record wins.
            None => {
                // No per-clause record: a document-scope default may cover it, but ONLY
                // for an authoring document AND a non-ambiguous clause (Decision 7).
                let ambiguous = unit.testability == Testability::Ambiguous;
                let covered = authoring_docs.contains(&unit.document)
                    && !ambiguous
                    && covered_docs.contains(&unit.document);
                if !covered {
                    join.unclassified.push(unit.id.clone());
                }
            }
        }
    }
    for cls in classifications {
        if let Some(clause) = cls.clause.as_deref() {
            if !unit_ids.contains(clause) {
                join.stale.push(cls.id.clone());
            }
        }
    }

    join.unclassified.sort();
    join.stale.sort();
    join.duplicated.sort();
    join
}

/// Enforce the normative-clause-inventory join classes (V11–V15) by joining the loaded
/// registry's clause-classification records against the committed clause inventory and the
/// vendored pinned prose (021-normative-clause-inventory). Returns every violation found
/// (sorted); an empty vector means the join is clean. The V11–V14 classes are the
/// *generalized* inventory-unit classes; V15 is prose-specific source integrity.
pub fn check_clause_inventory(registry: &Registry, inputs: &ClauseInputs) -> Vec<Violation> {
    let mut out: Vec<Violation> = Vec::new();

    // Scope out entirely when neither a committed clause inventory nor a vendored spec
    // directory exists (e.g. the V1–V10 acceptance fixtures). Not a silent fallback — the
    // real `conformance/` tree always has both, so the full V11–V15 set runs there.
    if !inputs.clauses_file.exists() && !inputs.spec_dir.exists() {
        return out;
    }

    // Load the spec manifest (for per-document scope) and the committed inventory.
    let manifest: Option<SpecManifest> = match load_spec_manifest(inputs.spec_dir) {
        Ok(m) => Some(m),
        Err(LoadError::SpecFingerprintMismatch {
            file,
            expected,
            actual,
        }) => {
            out.push(Violation::new(
                "V14",
                file.display().to_string(),
                format!(
                    "spec manifest fingerprint mismatch: manifest records {expected}, vendored \
                     file is {actual}"
                ),
            ));
            None
        }
        Err(e) => {
            out.push(Violation::new(
                "V14",
                inputs.spec_dir.display().to_string(),
                format!("could not load the spec manifest for the clause join: {e}"),
            ));
            None
        }
    };

    let committed: Option<ClauseInventory> = match load_clause_inventory(inputs.clauses_file) {
        Ok(inv) => inv,
        Err(e) => {
            out.push(Violation::new(
                "V14",
                inputs.clauses_file.display().to_string(),
                format!("could not load the committed clause inventory for the join: {e}"),
            ));
            None
        }
    };

    let units: &[ClauseUnit] = committed
        .as_ref()
        .map(|c| c.units.as_slice())
        .unwrap_or(&[]);
    let behavior_ids: HashSet<&str> = registry.behaviors.iter().map(|b| b.id.as_str()).collect();

    // Document scope + which authoring documents actually carry a valid doc-scope default.
    let authoring_docs: HashSet<String> = manifest
        .as_ref()
        .map(|m| {
            m.documents
                .iter()
                .filter(|d| d.scope == DocumentScope::Authoring)
                .map(|d| d.key.clone())
                .collect()
        })
        .unwrap_or_default();
    let all_docs: HashSet<String> = manifest
        .as_ref()
        .map(|m| m.documents.iter().map(|d| d.key.clone()).collect())
        .unwrap_or_default();
    let covered_docs: HashSet<String> = registry
        .clause_classifications
        .iter()
        .filter_map(|c| c.document.clone())
        .filter(|d| authoring_docs.contains(d))
        .collect();

    let join = join_clauses(
        units,
        &registry.clause_classifications,
        &authoring_docs,
        &covered_docs,
    );

    // V11 (stale per-clause), from the shared join.
    let stale: HashSet<&str> = join.stale.iter().map(String::as_str).collect();
    for cls in &registry.clause_classifications {
        if stale.contains(cls.id.as_str()) {
            out.push(Violation::new(
                "V11",
                &cls.id,
                format!(
                    "clause classification references clause {:?}, which is absent from the \
                     committed inventory (stale — delete or re-point it)",
                    cls.clause.as_deref().unwrap_or("")
                ),
            ));
        }
        // V13 (shape/linkage), per classification record.
        check_clause_classification_shape(cls, &behavior_ids, &authoring_docs, &all_docs, &mut out);
    }

    // V12 (unclassified / duplicated), per clause unit.
    for id in &join.unclassified {
        out.push(Violation::new(
            "V12",
            id,
            "clause has no effective disposition (unclassified — a per-clause classification \
             is required, or a document-scope default for a non-ambiguous authoring clause)",
        ));
    }
    for (id, cls_ids) in &join.duplicated {
        out.push(Violation::new(
            "V12",
            id,
            format!(
                "clause is classified by {} per-clause records ({}); exactly one is required \
                 (duplicated)",
                cls_ids.len(),
                cls_ids.join(", ")
            ),
        ));
    }

    // V15 (clause↔source integrity), per clause unit — needs the parsed prose.
    if let (Some(manifest), Some(committed)) = (manifest.as_ref(), committed.as_ref()) {
        check_clause_source_integrity(manifest, inputs.spec_dir, committed, &mut out);
    }

    // V14 (provenance): fingerprint (surfaced above), revision pin, and byte-identity.
    check_clause_provenance(registry, inputs, committed.as_ref(), &mut out);

    sort_violations(&mut out);
    out
}

/// V13 shape/linkage checks for a single clause-classification record
/// (contracts/clause-classification-schema.md).
fn check_clause_classification_shape(
    cls: &ClauseClassification,
    behavior_ids: &HashSet<&str>,
    authoring_docs: &HashSet<String>,
    all_docs: &HashSet<String>,
    out: &mut Vec<Violation>,
) {
    // `clause` XOR `document`.
    match (&cls.clause, &cls.document) {
        (Some(clause), None) => {
            // Per-clause: id = `clc-` + the exact tail of the `clu-` id.
            match clause.strip_prefix("clu-") {
                Some(tail) => {
                    let expected = format!("clc-{tail}");
                    if cls.id != expected {
                        out.push(Violation::new(
                            "V13",
                            &cls.id,
                            format!(
                                "clause classification id must mirror its clause tail (expected \
                                 {expected:?} for clause {clause:?})"
                            ),
                        ));
                    }
                }
                None => out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!(
                        "clause {clause:?} is not a `clu-` id; the id-tail mirror is undefined"
                    ),
                )),
            }
        }
        (None, Some(document)) => {
            // Document-scope: id = `clc-doc-<key>`, only for an authoring document.
            let expected = format!("clc-doc-{document}");
            if cls.id != expected {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!("document-scope classification id must be {expected:?}"),
                ));
            }
            if !all_docs.contains(document) {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!("document-scope classification targets unknown document {document:?}"),
                ));
            } else if !authoring_docs.contains(document) {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!(
                        "document-scope classification targets consumer document {document:?}; \
                         only authoring documents may carry a document-scope default"
                    ),
                ));
            }
        }
        (Some(_), Some(_)) => out.push(Violation::new(
            "V13",
            &cls.id,
            "clause classification has BOTH `clause` and `document` (exactly one is required)",
        )),
        (None, None) => out.push(Violation::new(
            "V13",
            &cls.id,
            "clause classification has NEITHER `clause` nor `document` (exactly one is required)",
        )),
    }

    // `behaviors` arity + existence, keyed to disposition.
    match cls.disposition {
        Disposition::BehaviorMapped => {
            if cls.behaviors.is_empty() {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    "disposition `behavior-mapped` requires a non-empty `behaviors` list",
                ));
            }
            for behavior in &cls.behaviors {
                if !behavior_ids.contains(behavior.as_str()) {
                    out.push(Violation::new(
                        "V13",
                        &cls.id,
                        format!(
                            "maps to behavior {behavior:?}, which is not an existing `bhv-` record"
                        ),
                    ));
                }
            }
        }
        Disposition::NonTestable | Disposition::NotApplicable => {
            if !cls.behaviors.is_empty() {
                out.push(Violation::new(
                    "V13",
                    &cls.id,
                    format!(
                        "disposition {} must have an empty `behaviors` list",
                        disposition_name(cls.disposition)
                    ),
                ));
            }
        }
    }

    // `non-testable` / `not-applicable` require a non-empty `rationale`.
    if matches!(
        cls.disposition,
        Disposition::NonTestable | Disposition::NotApplicable
    ) {
        let has_rationale = cls
            .rationale
            .as_deref()
            .is_some_and(|r| !r.trim().is_empty());
        if !has_rationale {
            out.push(Violation::new(
                "V13",
                &cls.id,
                format!(
                    "disposition {} requires a non-empty `rationale`",
                    disposition_name(cls.disposition)
                ),
            ));
        }
    }
}

/// V15 clause↔source integrity: strength ↔ excerpt keyword agreement, descriptive clauses
/// not hiding a mandatory keyword, and each excerpt present under its recorded heading.
fn check_clause_source_integrity(
    manifest: &SpecManifest,
    spec_dir: &Path,
    committed: &ClauseInventory,
    out: &mut Vec<Violation>,
) {
    // Parse each vendored document once.
    let mut docs: HashMap<&str, Document> = HashMap::new();
    for doc in &manifest.documents {
        let path = spec_dir.join(&doc.file);
        if let Ok(raw) = std::fs::read_to_string(&path) {
            docs.insert(doc.key.as_str(), Document::parse(&raw));
        }
    }

    for unit in &committed.units {
        // Strength ↔ keyword agreement over the FIRST location's excerpt (all locations
        // of a unit share the same normalized substance, so any is representative).
        if let Some(loc) = unit.locations.first() {
            match unit.strength {
                Strength::Must | Strength::Should | Strength::May => {
                    if !has_family(&loc.excerpt, unit.strength) {
                        out.push(Violation::new(
                            "V15",
                            &unit.id,
                            format!(
                                "strength label {:?} is not supported by the excerpt's RFC-2119 \
                                 keywords",
                                strength_name(unit.strength)
                            ),
                        ));
                    }
                }
                Strength::Descriptive => {
                    if hides_mandatory_keyword(&loc.excerpt) {
                        out.push(Violation::new(
                            "V15",
                            &unit.id,
                            "descriptive clause hides an unqualified mandatory RFC-2119 keyword",
                        ));
                    }
                }
                Strength::Algorithm | Strength::IoContract => {}
            }
        }
        // Every location's excerpt MUST be present in the pinned document under its anchor.
        let Some(doc) = docs.get(unit.document.as_str()) else {
            out.push(Violation::new(
                "V15",
                &unit.id,
                format!("clause document {:?} has no vendored prose", unit.document),
            ));
            continue;
        };
        for loc in &unit.locations {
            if !doc.contains_excerpt_at(&loc.anchor, &loc.excerpt) {
                out.push(Violation::new(
                    "V15",
                    &unit.id,
                    format!(
                        "excerpt not present in the pinned document under heading {:?} (anchor {:?})",
                        loc.heading, loc.anchor
                    ),
                ));
            }
        }
    }
}

/// V14 provenance for clauses: byte-identity vs canonicalized regeneration, and the
/// inventory revision ↔ registry `spec`-kind revision pin. (The spec-manifest fingerprint
/// is surfaced by [`check_clause_inventory`] via [`load_spec_manifest`].)
fn check_clause_provenance(
    registry: &Registry,
    inputs: &ClauseInputs,
    committed: Option<&ClauseInventory>,
    out: &mut Vec<Violation>,
) {
    match generate_clauses(inputs.spec_dir, inputs.clauses_file) {
        Ok(regenerated) => match std::fs::read_to_string(inputs.clauses_file) {
            Ok(committed_raw) => {
                if committed_raw != render_clauses(&regenerated) {
                    out.push(Violation::new(
                        "V14",
                        inputs.clauses_file.display().to_string(),
                        "committed clause inventory does not byte-match a fresh canonicalization \
                         from the pinned prose (run `clause generate`)",
                    ));
                }
            }
            Err(e) => out.push(Violation::new(
                "V14",
                inputs.clauses_file.display().to_string(),
                format!("could not read the committed clause inventory for comparison: {e}"),
            )),
        },
        // A fingerprint/integrity error is already reported (fingerprint by
        // check_clause_inventory; excerpt/strength by V15). Report other regeneration
        // failures as provenance breakage.
        Err(
            LoadError::SpecFingerprintMismatch { .. }
            | LoadError::ExcerptNotFoundAtAnchor { .. }
            | LoadError::StrengthKeywordMismatch { .. },
        ) => {}
        Err(e) => out.push(Violation::new(
            "V14",
            inputs.spec_dir.display().to_string(),
            format!("could not regenerate the clause inventory from the pinned prose: {e}"),
        )),
    }

    if let Some(committed) = committed {
        let spec_revisions: Vec<&str> = registry
            .revisions
            .iter()
            .filter(|r| r.kind == RevisionKind::Spec)
            .map(|r| r.id.as_str())
            .collect();
        if spec_revisions.is_empty() {
            out.push(Violation::new(
                "V14",
                committed.revision.clone(),
                "the registry declares no `spec`-kind revision to pin the clause inventory against",
            ));
        } else if !spec_revisions.contains(&committed.revision.as_str()) {
            out.push(Violation::new(
                "V14",
                committed.revision.clone(),
                format!(
                    "clause inventory revision {:?} names no `spec`-kind revision record (declared: \
                     {})",
                    committed.revision,
                    spec_revisions.join(", ")
                ),
            ));
        }
    }
}

fn strength_name(strength: Strength) -> &'static str {
    match strength {
        Strength::Must => "must",
        Strength::Should => "should",
        Strength::May => "may",
        Strength::Algorithm => "algorithm",
        Strength::IoContract => "io-contract",
        Strength::Descriptive => "descriptive",
    }
}

/// Convert the loader's located schema errors into `SCHEMA`-class violations. The
/// `record` field carries the file path (with `line:column` when the loader
/// captured one) so a schema failure names its location like every other violation.
pub fn schema_violations(errors: &[SchemaError]) -> Vec<Violation> {
    let mut out: Vec<Violation> = errors
        .iter()
        .map(|e| {
            let record = match &e.location {
                Some(loc) => format!("{}:{}", e.file.display(), loc),
                None => e.file.display().to_string(),
            };
            Violation::new("SCHEMA", record, e.message.clone())
        })
        .collect();
    sort_violations(&mut out);
    out
}

/// Run every violation-class check (V1–V10) over a loaded registry and return ALL
/// violations, sorted by code then record ID (FR-019, contracts/cli.md). Assumes the
/// registry already parsed cleanly (schema failures are handled by [`validate_path`]).
pub fn run(registry: &Registry, today: &str, repo_root: &Path) -> Vec<Violation> {
    let mut checker = Checker::new(registry, repo_root);
    // V2 before V1 so a duplicate/misprefixed id is reported on its own terms even
    // when the same record also participates in a dangling reference.
    checker.check_ids_and_duplicates(); // V2
    checker.check_references(); // V1 (record refs, dimension values, orphan behaviors)
    checker.check_executables(); // V1 (executable test binaries)
    checker.check_case_behaviors(); // V3
    checker.check_source_scope(); // V4
    checker.check_coverage(); // V5
    checker.check_waiver_expiry(today); // V6
    checker.check_pins(); // V7
    checker.check_contradictions(); // V8
    checker.check_channels(); // V9
    checker.check_context_intersection(); // V10
    checker.check_declarative_cases(); // V16 (022-conformance-runner)
    checker.check_docker_pinned_inputs(); // V18 (022-conformance-runner US5)
    checker.check_allowed_difference_refs(); // V19 (022-conformance-runner US4)
    checker.check_metamorphic_arity(); // V20 (022-conformance-runner US6)

    let mut out = checker.violations;
    sort_violations(&mut out);
    out
}

/// Deterministic order: code rank, then record ID, then message.
fn sort_violations(violations: &mut [Violation]) {
    violations.sort_by(|a, b| {
        code_rank(&a.code)
            .cmp(&code_rank(&b.code))
            .then_with(|| a.record.cmp(&b.record))
            .then_with(|| a.message.cmp(&b.message))
    });
}

/// Cross-reference indices + accumulated violations for one validation run.
struct Checker<'a> {
    reg: &'a Registry,
    repo_root: &'a Path,
    violations: Vec<Violation>,

    /// Behavior id → the behavior (for R-rule and V10 lookups).
    behaviors: HashMap<&'a str, &'a BehaviorUnit>,
    /// Declared revision ids.
    revision_ids: HashSet<&'a str>,
    /// Dimension id → its declared value set.
    dim_values: HashMap<&'a str, HashSet<&'a str>>,
    /// Declared channel ids.
    channel_ids: HashSet<&'a str>,
    /// The single active profile, if any (coverage/in-profile checks no-op without
    /// one — a registry with zero behaviors validates cleanly).
    active_profile: Option<&'a CertificationProfile>,

    /// Behavior ids covered by ≥1 test case / waiver / gap (structural coverage).
    covered_by_case: HashSet<&'a str>,
    covered_by_waiver: HashSet<&'a str>,
    covered_by_gap: HashSet<&'a str>,
    /// Behavior ids referenced by ≥1 source unit (every behavior needs one — V1).
    sourced: HashSet<&'a str>,
}

impl<'a> Checker<'a> {
    fn new(reg: &'a Registry, repo_root: &'a Path) -> Checker<'a> {
        let behaviors: HashMap<&str, &BehaviorUnit> =
            reg.behaviors.iter().map(|b| (b.id.as_str(), b)).collect();
        let revision_ids: HashSet<&str> = reg.revisions.iter().map(|r| r.id.as_str()).collect();
        let dim_values: HashMap<&str, HashSet<&str>> = reg
            .dimensions
            .iter()
            .map(|d| (d.id.as_str(), d.values.iter().map(String::as_str).collect()))
            .collect();
        let channel_ids: HashSet<&str> = reg.channels.iter().map(|c| c.id.as_str()).collect();
        let active_profile = reg.profiles.iter().find(|p| p.active);

        let mut covered_by_case = HashSet::new();
        for case in &reg.cases {
            for b in &case.behaviors {
                covered_by_case.insert(b.as_str());
            }
        }
        let mut covered_by_waiver = HashSet::new();
        for waiver in &reg.waivers {
            for b in &waiver.behaviors {
                covered_by_waiver.insert(b.as_str());
            }
        }
        let mut covered_by_gap = HashSet::new();
        for gap in &reg.gaps {
            for b in &gap.behaviors {
                covered_by_gap.insert(b.as_str());
            }
        }
        let mut sourced = HashSet::new();
        for src in &reg.sources {
            for b in &src.behaviors {
                sourced.insert(b.as_str());
            }
        }
        // A behavior-mapped clause classification is ALSO provenance (021 T036, FR-026):
        // once a prose clause maps to a behavior via a `clc-` record, the behavior
        // back-traces to the pinned prose through that clause, so the hand-written
        // `src-spec-*` prose source unit is redundant and may be retired without orphaning
        // the behavior. This is the single-path traceability the retirement establishes.
        for clc in &reg.clause_classifications {
            if matches!(clc.disposition, Disposition::BehaviorMapped) {
                for b in &clc.behaviors {
                    sourced.insert(b.as_str());
                }
            }
        }

        Checker {
            reg,
            repo_root,
            violations: Vec::new(),
            behaviors,
            revision_ids,
            dim_values,
            channel_ids,
            active_profile,
            covered_by_case,
            covered_by_waiver,
            covered_by_gap,
            sourced,
        }
    }

    fn push(&mut self, code: &str, record: impl Into<String>, message: impl Into<String>) {
        self.violations.push(Violation::new(code, record, message));
    }

    // -- V2: duplicate ids, ID format, prefix↔type agreement -----------------

    fn check_ids_and_duplicates(&mut self) {
        let mut seen: HashSet<String> = HashSet::new();
        for (id, ty) in self.all_ids() {
            match parse_id(id) {
                Ok(parsed) if parsed == ty => {}
                Ok(parsed) => self.push(
                    "V2",
                    id,
                    format!(
                        "id prefix denotes a {} record but it appears as a {} record \
                         (prefix↔type mismatch, FR-004)",
                        record_type_name(parsed),
                        record_type_name(ty),
                    ),
                ),
                Err(err) => self.push("V2", id, err.to_string()),
            }
            if !seen.insert(id.to_string()) {
                self.push(
                    "V2",
                    id,
                    "duplicate stable id (ids must be unique across the whole registry)",
                );
            }
        }
    }

    /// Every `(id, record type)` pair in the registry, in collection order.
    fn all_ids(&self) -> Vec<(&'a str, RecordType)> {
        let mut out: Vec<(&str, RecordType)> = Vec::new();
        let r = self.reg;
        out.extend(
            r.revisions
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Revision)),
        );
        out.extend(
            r.sources
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Source)),
        );
        out.extend(
            r.dimensions
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Dimension)),
        );
        out.extend(
            r.channels
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Channel)),
        );
        out.extend(
            r.profiles
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Profile)),
        );
        out.extend(
            r.behaviors
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Behavior)),
        );
        out.extend(r.cases.iter().map(|x| (x.id.as_str(), RecordType::Case)));
        out.extend(r.gaps.iter().map(|x| (x.id.as_str(), RecordType::Gap)));
        out.extend(
            r.waivers
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Waiver)),
        );
        out.extend(
            r.extensions
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Extension)),
        );
        // Classification records are registry records like any other, so the V2 id
        // grammar / uniqueness / prefix↔type checks cover them too (data-model.md §5).
        out.extend(
            r.classifications
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Classification)),
        );
        out
    }

    // -- V1: dangling references, dimension values, orphan behaviors ---------

    fn check_references(&mut self) {
        // Source units: revision provenance + behavior links.
        for src in &self.reg.sources {
            if !self.revision_ids.contains(src.revision.as_str()) {
                self.push(
                    "V1",
                    &src.id,
                    format!("references undefined source revision {:?}", src.revision),
                );
            }
            self.check_behavior_refs(&src.id, &src.behaviors);
        }

        // Active/declared profiles: context assigns declared dimension→value pairs.
        for profile in &self.reg.profiles {
            for (dim, val) in &profile.context {
                self.check_dim_value(&profile.id, dim, val);
            }
        }

        // Behaviors: applicability dimensions + values.
        for bhv in &self.reg.behaviors {
            self.check_conditions(&bhv.id, &bhv.applicability);
        }

        // Cases, gaps, waivers, extensions: behavior links (+ case context dims).
        for case in &self.reg.cases {
            self.check_behavior_refs(&case.id, &case.behaviors);
            self.check_conditions(&case.id, &case.context);
        }
        for gap in &self.reg.gaps {
            self.check_behavior_refs(&gap.id, &gap.behaviors);
        }
        for waiver in &self.reg.waivers {
            self.check_behavior_refs(&waiver.id, &waiver.behaviors);
        }
        for ext in &self.reg.extensions {
            self.check_behavior_refs(&ext.id, &ext.behaviors);
        }

        // Orphan behaviors: every behavior must be referenced by ≥1 source unit
        // (data-model BehaviorUnit.sources is the inverse of SourceUnit.behaviors).
        for bhv in &self.reg.behaviors {
            if !self.sourced.contains(bhv.id.as_str()) {
                self.push(
                    "V1",
                    &bhv.id,
                    "behavior is referenced by no source unit (every behavior needs ≥1 source)",
                );
            }
        }
    }

    fn check_behavior_refs(&mut self, from: &str, refs: &[String]) {
        for target in refs {
            if !self.behaviors.contains_key(target.as_str()) {
                self.push(
                    "V1",
                    from,
                    format!("references undefined behavior {target:?}"),
                );
            }
        }
    }

    fn check_conditions(&mut self, from: &str, conditions: &[Condition]) {
        for cond in conditions {
            for val in &cond.values {
                self.check_dim_value(from, &cond.dimension, val);
            }
            if cond.values.is_empty() {
                // A dimension pinned to no values still references the dimension id.
                self.check_dim_exists(from, &cond.dimension);
            }
        }
    }

    fn check_dim_exists(&mut self, from: &str, dim: &str) {
        if !self.dim_values.contains_key(dim) {
            self.push(
                "V1",
                from,
                format!("references undeclared context dimension {dim:?}"),
            );
        }
    }

    fn check_dim_value(&mut self, from: &str, dim: &str, val: &str) {
        match self.dim_values.get(dim) {
            None => self.push(
                "V1",
                from,
                format!("references undeclared context dimension {dim:?}"),
            ),
            Some(values) if !values.contains(val) => self.push(
                "V1",
                from,
                format!("references undeclared value {val:?} of dimension {dim:?}"),
            ),
            Some(_) => {}
        }
    }

    // -- V1: executable test binaries exist ----------------------------------

    fn check_executables(&mut self) {
        for case in &self.reg.cases {
            // Declarative cases (022-conformance-runner) carry no `executable`; they
            // are run by the shared runner, not a bespoke Rust binary.
            let Some(executable) = &case.executable else {
                continue;
            };
            let binary = &executable.binary;
            if !binary_exists(self.repo_root, binary) {
                self.push(
                    "V1",
                    &case.id,
                    format!(
                        "executable references test binary {binary:?}, but no \
                         crates/*/tests/{binary}.rs exists"
                    ),
                );
            }
        }
    }

    // -- V3: test case with no behaviors -------------------------------------

    fn check_case_behaviors(&mut self) {
        for case in &self.reg.cases {
            if case.behaviors.is_empty() {
                self.push(
                    "V3",
                    &case.id,
                    "test case is linked to no behavior (orphan case)",
                );
            }
        }
    }

    // -- V4: source unit with empty behaviors and no outOfScope --------------

    fn check_source_scope(&mut self) {
        for src in &self.reg.sources {
            if src.behaviors.is_empty() && src.out_of_scope.is_none() {
                self.push(
                    "V4",
                    &src.id,
                    "source unit has no behaviors and no `outOfScope` classification",
                );
            }
        }
    }

    // -- V5: in-active-profile behavior with no case, waiver, or gap ---------

    fn check_coverage(&mut self) {
        let Some(profile) = self.active_profile else {
            return; // No active profile → nothing is "in profile" to require coverage.
        };
        for bhv in &self.reg.behaviors {
            if !applies_in_profile(bhv, profile) {
                continue;
            }
            let id = bhv.id.as_str();
            let covered = self.covered_by_case.contains(id)
                || self.covered_by_waiver.contains(id)
                || self.covered_by_gap.contains(id);
            if !covered {
                self.push(
                    "V5",
                    id,
                    format!(
                        "behavior is applicable in the active profile {:?} but has no \
                         test case, waiver, or gap",
                        profile.id
                    ),
                );
            }
        }
    }

    // -- V6: waiver expiry (ISO lexicographic; boundary expires==today passes) --

    fn check_waiver_expiry(&mut self, today: &str) {
        for waiver in &self.reg.waivers {
            // `added`/`expires` MUST be canonical `YYYY-MM-DD`. The expiry test is a
            // lexicographic string compare, which is only sound for zero-padded ISO
            // dates: a non-canonical date (e.g. `"2026-7-1"`) sorts wrong and would
            // let an actually-expired waiver slip past validation — and, through the
            // `certify` gate, past the release. Reject it explicitly rather than
            // silently mis-evaluating expiry (constitution IV: fail fast).
            if !is_canonical_date(&waiver.added) {
                self.push(
                    "V6",
                    &waiver.id,
                    format!(
                        "waiver `added` is not a canonical YYYY-MM-DD date: {:?}",
                        waiver.added
                    ),
                );
            }
            if !is_canonical_date(&waiver.expires) {
                self.push(
                    "V6",
                    &waiver.id,
                    format!(
                        "waiver `expires` is not a canonical YYYY-MM-DD date: {:?}",
                        waiver.expires
                    ),
                );
            } else if waiver.expires.as_str() < today {
                self.push(
                    "V6",
                    &waiver.id,
                    format!(
                        "waiver expired: expires {:?} is earlier than today {today:?}",
                        waiver.expires
                    ),
                );
            }
        }
    }

    // -- V7: source revision pin vs verifiedAgainst repo file ----------------

    fn check_pins(&mut self) {
        for rev in &self.reg.revisions {
            let Some(rel) = &rev.verified_against else {
                continue;
            };
            let path = self.repo_root.join(rel);
            match read_verified_pin(&path) {
                Ok(version) if version == rev.pin => {}
                Ok(version) => self.push(
                    "V7",
                    &rev.id,
                    format!(
                        "pin {:?} disagrees with {rel:?} version {version:?}",
                        rev.pin
                    ),
                ),
                Err(cause) => self.push(
                    "V7",
                    &rev.id,
                    format!("cannot verify pin against {rel:?}: {cause}"),
                ),
            }
        }
    }

    // -- V8: disposition contradictions (rules R1–R8) ------------------------

    fn check_contradictions(&mut self) {
        for bhv in &self.reg.behaviors {
            self.check_behavior_rules(bhv);
        }
        // Extension↔decision consistency (part of V8): every behavior linked from a
        // DeaconExtension must carry `decision: deacon-extension`.
        for ext in &self.reg.extensions {
            for target in &ext.behaviors {
                if let Some(bhv) = self.behaviors.get(target.as_str()) {
                    if bhv.decision != Decision::DeaconExtension {
                        self.push(
                            "V8",
                            &ext.id,
                            format!(
                                "extension links behavior {:?}, which has decision {} \
                                 (extensions require decision `deacon-extension`)",
                                bhv.id,
                                decision_name(bhv.decision),
                            ),
                        );
                    }
                }
            }
        }
    }

    fn check_behavior_rules(&mut self, bhv: &BehaviorUnit) {
        let id = bhv.id.as_str();
        let spec = bhv.spec;
        let reference = bhv.reference;
        let decision = bhv.decision;
        let has_case = self.covered_by_case.contains(id);
        let has_waiver = self.covered_by_waiver.contains(id);
        let has_gap = self.covered_by_gap.contains(id);
        let gap_only = has_gap && !has_case && !has_waiver;
        let in_profile = self
            .active_profile
            .is_some_and(|p| applies_in_profile(bhv, p));

        // R1: `unresolved-gap` contradicts (spec conformant AND reference aligned).
        if decision == Decision::UnresolvedGap
            && spec == SpecStatus::Conformant
            && reference == ReferenceStatus::Aligned
        {
            self.push(
                "V8",
                id,
                "R1: decision `unresolved-gap` contradicts spec `conformant` + reference `aligned`",
            );
        }
        // R2: `deacon-extension` requires spec ∈ {unspecified, not-applicable}.
        if decision == Decision::DeaconExtension
            && !matches!(spec, SpecStatus::Unspecified | SpecStatus::NotApplicable)
        {
            self.push(
                "V8",
                id,
                format!(
                    "R2: decision `deacon-extension` requires spec `unspecified` or \
                     `not-applicable`, found {}",
                    spec_name(spec)
                ),
            );
        }
        // R3: `intentional-divergence` contradicts reference `aligned`.
        if decision == Decision::IntentionalDivergence && reference == ReferenceStatus::Aligned {
            self.push(
                "V8",
                id,
                "R3: decision `intentional-divergence` contradicts reference `aligned`",
            );
        }
        // R4: reference `unknown` on an in-profile behavior requires `unresolved-gap`.
        if in_profile
            && reference == ReferenceStatus::Unknown
            && decision != Decision::UnresolvedGap
        {
            self.push(
                "V8",
                id,
                format!(
                    "R4: in-profile reference `unknown` requires decision `unresolved-gap`, \
                     found {}",
                    decision_name(decision)
                ),
            );
        }
        // R5: `follow-spec` requires spec `conformant`.
        if decision == Decision::FollowSpec && spec != SpecStatus::Conformant {
            self.push(
                "V8",
                id,
                format!(
                    "R5: decision `follow-spec` requires spec `conformant`, found {}",
                    spec_name(spec)
                ),
            );
        }
        // R6: `align-with-reference` requires reference `aligned`.
        if decision == Decision::AlignWithReference && reference != ReferenceStatus::Aligned {
            self.push(
                "V8",
                id,
                format!(
                    "R6: decision `align-with-reference` requires reference `aligned`, found {}",
                    reference_name(reference)
                ),
            );
        }
        // R7: a behavior whose only structural coverage is a gap requires
        // `unresolved-gap`.
        if gap_only && decision != Decision::UnresolvedGap {
            self.push(
                "V8",
                id,
                format!(
                    "R7: gap-only coverage requires decision `unresolved-gap`, found {}",
                    decision_name(decision)
                ),
            );
        }
        // R8: an in-profile behavior with no case AND no waiver requires reference
        // `unknown` (statuses are verified claims, not aspirations).
        //
        // Deacon extensions are EXEMPT: for a `deacon-extension`, `not-applicable`
        // is the correct reference status (the reference CLI has no concept of the
        // behavior — it is a classification, not an unverified claim), so forcing
        // `unknown` would be wrong. This exemption is also belt-and-suspenders: R2
        // already constrains an extension's spec, and R7 blocks gap-only extensions,
        // so any VALID in-profile extension is already case- or waiver-backed (which
        // makes R8's antecedent false regardless of the exemption). Encoded here so
        // the rule reads consistently and never double-reports.
        if in_profile
            && !has_case
            && !has_waiver
            && reference != ReferenceStatus::Unknown
            && decision != Decision::DeaconExtension
        {
            self.push(
                "V8",
                id,
                format!(
                    "R8: in-profile behavior with no case and no waiver requires reference \
                     `unknown`, found {}",
                    reference_name(reference)
                ),
            );
        }
    }

    // -- V9: outcome/expected referencing an undeclared observable channel ---

    fn check_channels(&mut self) {
        for case in &self.reg.cases {
            // Legacy outcomes.
            for outcome in &case.outcomes {
                if !self.channel_ids.contains(outcome.channel.as_str()) {
                    self.push(
                        "V9",
                        &case.id,
                        format!(
                            "outcome references undeclared observable channel {:?}",
                            outcome.channel
                        ),
                    );
                }
            }
            // Declarative expectations (022-conformance-runner): every `expected[].channel`
            // must be declared in `channels.json` too (contract case-schema.md → V9).
            for exp in &case.expected {
                if !self.channel_ids.contains(exp.channel.as_str()) {
                    self.push(
                        "V9",
                        &case.id,
                        format!(
                            "expected observable references undeclared observable channel {:?}",
                            exp.channel
                        ),
                    );
                }
            }
        }
    }

    // -- V10: case context vs linked behavior applicability intersection -----

    fn check_context_intersection(&mut self) {
        for case in &self.reg.cases {
            for target in &case.behaviors {
                let Some(bhv) = self.behaviors.get(target.as_str()) else {
                    continue; // dangling behavior ref is a V1 concern.
                };
                if let Some(dim) = conflicting_dimension(&case.context, &bhv.applicability) {
                    self.push(
                        "V10",
                        &case.id,
                        format!(
                            "case context and behavior {:?} applicability have an empty \
                             intersection on dimension {dim:?}",
                            bhv.id
                        ),
                    );
                }
            }
        }
    }

    // -- V16: declarative case well-formedness (022-conformance-runner, T013) -----
    //
    // Core well-formedness of a declarative case record (data-model §1, contract
    // case-schema.md). The exactly-one-of shape is normally caught fail-loud at load
    // (`load.rs::check_case_shapes`); it is re-checked here so a directly-constructed
    // `Registry` (e.g. a unit test that bypasses `Registry::load`) still surfaces it
    // under a stable code. Legacy (binary-backed) cases are governed by V1/V3/V9/V10
    // and are untouched here.
    fn check_declarative_cases(&mut self) {
        for case in &self.reg.cases {
            match case.classify() {
                Err(shape) => self.push("V16", &case.id, shape.message().to_string()),
                Ok(CaseKind::Legacy) => {} // legacy cases: existing V-series apply.
                Ok(CaseKind::Declarative) => self.check_one_declarative_case(case),
            }
        }
    }

    /// The declarative-only rules, evaluated on a record already classified
    /// [`CaseKind::Declarative`]: `oracleType` present, every `operations[].subcommand`
    /// in the consumer surface (Principle II), `spec-expectation` ⇒ every `expected`
    /// carries an `assertion`, and `fsAllowlist` present **iff** a filesystem-channel
    /// expectation exists.
    fn check_one_declarative_case(&mut self, case: &crate::model::TestCase) {
        // oracleType must be declared (its 4-value membership is enforced at load by
        // the closed `OracleType` enum; here we require presence).
        let Some(oracle_type) = case.oracle_type else {
            self.push(
                "V16",
                &case.id,
                "declarative case must declare an `oracleType` \
                 (spec-expectation | snapshot | live-differential | invariant-metamorphic)",
            );
            // Continue: the remaining rules do not depend on the oracle type except
            // the spec-expectation assertion rule, which we can skip safely.
            self.check_case_subcommands(case);
            self.check_case_fs_allowlist(case);
            return;
        };

        self.check_case_subcommands(case);

        // spec-expectation ⇒ every declared expectation carries an assertion (there is
        // no reference/snapshot to supply it).
        if oracle_type == OracleType::SpecExpectation {
            for exp in &case.expected {
                if exp.assertion.is_none() {
                    self.push(
                        "V16",
                        &case.id,
                        format!(
                            "spec-expectation case must carry an `assertion` on every \
                             expected channel; channel {:?} has none",
                            exp.channel
                        ),
                    );
                }
            }
        }

        self.check_case_fs_allowlist(case);
    }

    /// Every `operations[].subcommand` must be in the consumer surface (Principle II).
    fn check_case_subcommands(&mut self, case: &crate::model::TestCase) {
        for op in &case.operations {
            if !CONSUMER_SUBCOMMANDS.contains(&op.subcommand.as_str()) {
                self.push(
                    "V16",
                    &case.id,
                    format!(
                        "operation {:?} references non-consumer subcommand {:?}; the \
                         consumer surface is {}",
                        op.id,
                        op.subcommand,
                        CONSUMER_SUBCOMMANDS.join(" | ")
                    ),
                );
            }
        }
    }

    // -- V20: invariant-metamorphic arity (022-conformance-runner US6, T070) ----------
    //
    // An `invariant-metamorphic` case MUST declare ≥2 operations and at least one
    // operation with a `relationship` referencing a SIBLING op id (data-model §1, FR-008)
    // — a relationship needs a second operation to relate to. Conversely, a `relationship`
    // on a non-metamorphic case is meaningless (only the metamorphic oracle evaluates it).
    fn check_metamorphic_arity(&mut self) {
        use crate::model::OracleType;
        for case in &self.reg.cases {
            let op_ids: HashSet<&str> = case.operations.iter().map(|o| o.id.as_str()).collect();
            let is_metamorphic = case.oracle_type == Some(OracleType::InvariantMetamorphic);

            if is_metamorphic {
                if case.operations.len() < 2 {
                    self.push(
                        "V20",
                        &case.id,
                        "invariant-metamorphic case must declare at least 2 operations \
                         (a relationship needs a sibling to relate to)",
                    );
                }
                if !case.operations.iter().any(|o| o.relationship.is_some()) {
                    self.push(
                        "V20",
                        &case.id,
                        "invariant-metamorphic case must declare a `relationship` on at least \
                         one operation (the declared relationship IS the oracle)",
                    );
                }
            }

            for op in &case.operations {
                let Some(rel) = &op.relationship else {
                    continue;
                };
                if !is_metamorphic {
                    self.push(
                        "V20",
                        &case.id,
                        format!(
                            "operation {:?} declares a `relationship`, but the case is not \
                             invariant-metamorphic (only that oracle evaluates relationships)",
                            op.id
                        ),
                    );
                }
                if rel.against_op == op.id {
                    self.push(
                        "V20",
                        &case.id,
                        format!(
                            "operation {:?} relationship references itself; it must reference a \
                             SIBLING operation",
                            op.id
                        ),
                    );
                } else if !op_ids.contains(rel.against_op.as_str()) {
                    self.push(
                        "V20",
                        &case.id,
                        format!(
                            "operation {:?} relationship references op {:?}, which is not an \
                             operation of the case",
                            op.id, rel.against_op
                        ),
                    );
                }
            }
        }
    }

    // -- V19: allowed-difference identity resolution (022-conformance-runner US4, T063) --
    //
    // Each allowed difference must (a) scope to a behavior the case links, and (b) resolve
    // its backing identity — `waiverId` to an existing `wvr-` record, or `divergenceId` to
    // an existing `ext-` record or an intentional-divergence behavior (FR-031/043). The
    // exactly-one-of + bare-channel/duplicate structural checks are enforced fail-loud at
    // load (`load.rs::check_allowed_differences`); V19 covers cross-record resolution.
    fn check_allowed_difference_refs(&mut self) {
        let waiver_ids: HashSet<&str> = self.reg.waivers.iter().map(|w| w.id.as_str()).collect();
        let ext_ids: HashSet<&str> = self.reg.extensions.iter().map(|e| e.id.as_str()).collect();
        let intentional_behaviors: HashSet<&str> = self
            .reg
            .behaviors
            .iter()
            .filter(|b| b.decision == Decision::IntentionalDivergence)
            .map(|b| b.id.as_str())
            .collect();

        for case in &self.reg.cases {
            let linked: HashSet<&str> = case.behaviors.iter().map(String::as_str).collect();
            for ad in &case.allowed_differences {
                if !linked.contains(ad.behavior.as_str()) {
                    self.push(
                        "V19",
                        &case.id,
                        format!(
                            "allowed difference scopes to behavior {:?}, which the case does not \
                             link (a tolerance must be scoped to a linked behavior)",
                            ad.behavior
                        ),
                    );
                }
                if let Some(w) = &ad.waiver_id {
                    if !waiver_ids.contains(w.as_str()) {
                        self.push(
                            "V19",
                            &case.id,
                            format!(
                                "allowed difference waiverId {w:?} resolves to no \
                                 `conformance/registry/waivers/wvr-*` record (dangling tolerance)"
                            ),
                        );
                    }
                }
                if let Some(d) = &ad.divergence_id {
                    if !ext_ids.contains(d.as_str()) && !intentional_behaviors.contains(d.as_str())
                    {
                        self.push(
                            "V19",
                            &case.id,
                            format!(
                                "allowed difference divergenceId {d:?} resolves to no `ext-` \
                                 record or intentional-divergence behavior (dangling tolerance)"
                            ),
                        );
                    }
                }
            }
        }
    }

    // -- V18: Docker cases must pin their image inputs (022-conformance-runner, T058) --
    //
    // A Docker-backed case (`resourceGroup: docker-*`) must reference fixtures whose
    // devcontainer image is PINNED — a `@sha256:` digest or a concrete `:tag` other than
    // `latest` (FR-038). A floating `latest` (or an untagged image) makes a snapshot
    // non-reproducible. Fixtures whose config declares no `image` (Dockerfile/compose) or
    // is absent/unreadable are skipped — V18 flags only a declared, unpinned image.
    fn check_docker_pinned_inputs(&mut self) {
        let fixtures_root = self.repo_root.join("conformance").join("fixtures");
        for case in &self.reg.cases {
            if !is_docker_case(case) {
                continue;
            }
            let mut fixture_ids: Vec<&str> = case
                .operations
                .iter()
                .flat_map(|op| op.fixtures.iter().map(String::as_str))
                .collect();
            fixture_ids.sort_unstable();
            fixture_ids.dedup();
            for id in fixture_ids {
                if let Some(image) = fixture_image(&fixtures_root.join(id)) {
                    if !image_is_pinned(&image) {
                        self.violations.push(Violation::new(
                            "V18",
                            &case.id,
                            format!(
                                "Docker case references fixture {id:?} with an unpinned image \
                                 {image:?}; pin it to a digest (`@sha256:…`) or a concrete tag \
                                 (never `latest`) for reproducible snapshots (FR-038)"
                            ),
                        ));
                    }
                }
            }
        }
    }

    /// `fsAllowlist` must be non-empty **iff** an `expected` filesystem-channel exists —
    /// required so the filesystem observer has a scope, forbidden otherwise so capture
    /// stays scoped (data-model §1, clarify Q1).
    fn check_case_fs_allowlist(&mut self, case: &crate::model::TestCase) {
        let has_fs_expectation = case
            .expected
            .iter()
            .any(|e| FILESYSTEM_CHANNELS.contains(&e.channel.as_str()));
        match (has_fs_expectation, case.fs_allowlist.is_empty()) {
            (true, true) => self.push(
                "V16",
                &case.id,
                "case has a filesystem-channel expectation but declares no `fsAllowlist` \
                 (filesystem capture must be allowlist-scoped)",
            ),
            (false, false) => self.push(
                "V16",
                &case.id,
                "case declares an `fsAllowlist` but has no filesystem-channel expectation \
                 (remove the allowlist to keep capture scoped)",
            ),
            _ => {}
        }
    }
}

/// Whether a case is Docker-backed (its `resourceGroup` requests a Docker group).
fn is_docker_case(case: &crate::model::TestCase) -> bool {
    use crate::model::ResourceGroup;
    matches!(
        case.resource_group,
        Some(ResourceGroup::DockerShared) | Some(ResourceGroup::DockerExclusive)
    )
}

/// The `image` a fixture's devcontainer config declares, if any. Looks in
/// `<fixture>/.devcontainer/devcontainer.json` then `<fixture>/.devcontainer.json`. A
/// missing/unreadable/non-JSON config, or one with no `image`, yields `None`.
fn fixture_image(fixture_dir: &Path) -> Option<String> {
    for rel in [".devcontainer/devcontainer.json", ".devcontainer.json"] {
        let path = fixture_dir.join(rel);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if let Some(image) = doc.get("image").and_then(|v| v.as_str()) {
            return Some(image.to_string());
        }
    }
    None
}

/// Whether an image reference is PINNED (FR-038): a `@sha256:` digest, or a concrete
/// `:tag` other than `latest`. An untagged image or `:latest` is NOT pinned. The tag is
/// the segment after the LAST `/` (so a `registry:port/image` host-port colon is not
/// mistaken for a tag).
fn image_is_pinned(image: &str) -> bool {
    if image.contains("@sha256:") {
        return true;
    }
    let last_segment = image.rsplit('/').next().unwrap_or(image);
    match last_segment.split_once(':') {
        Some((_, tag)) => !tag.is_empty() && tag != "latest",
        None => false, // no tag → unpinned
    }
}

/// Whether a behavior applies in a profile's context: every applicability condition
/// must be satisfied by the profile's assignment (empty applicability = everywhere).
/// A condition on a dimension the profile does not assign is treated as unsatisfied.
pub fn applies_in_profile(behavior: &BehaviorUnit, profile: &CertificationProfile) -> bool {
    behavior.applicability.iter().all(|cond| {
        profile
            .context
            .get(&cond.dimension)
            .is_some_and(|assigned| cond.values.iter().any(|v| v == assigned))
    })
}

/// The first dimension on which two condition sets have an empty value intersection,
/// or `None` if they can be jointly satisfied. A dimension constrained by only one
/// side is unconstrained on the other (so it never conflicts); a conflict requires
/// BOTH sides to constrain the same dimension to disjoint value sets. This is the
/// applicability/context intersection evaluator shared with coverage (T007).
pub fn conflicting_dimension(a: &[Condition], b: &[Condition]) -> Option<String> {
    let a_map = conditions_to_map(a);
    let b_map = conditions_to_map(b);
    // Scan dimensions in a stable (sorted) order so that, when a case conflicts on
    // more than one dimension, the reported dimension is deterministic rather than
    // dependent on HashMap iteration order.
    let mut dims: Vec<&String> = a_map.keys().collect();
    dims.sort();
    for dim in dims {
        if let Some(b_values) = b_map.get(dim) {
            if a_map[dim].is_disjoint(b_values) {
                return Some(dim.clone());
            }
        }
    }
    None
}

/// Collapse a condition list into dimension → value-set (unioning repeated
/// dimensions), for intersection testing.
fn conditions_to_map(conditions: &[Condition]) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for cond in conditions {
        let entry = map.entry(cond.dimension.clone()).or_default();
        for v in &cond.values {
            entry.insert(v.clone());
        }
    }
    map
}

/// Whether a nextest test binary `binary` exists as a source file
/// `crates/<crate>/tests/<binary>.rs` under `repo_root` (research Decision 9,
/// mirroring `parity-harness::registry::check_test_files`). Uses `PathBuf` joins
/// only — separator-agnostic, so the check behaves identically on Windows.
fn binary_exists(repo_root: &Path, binary: &str) -> bool {
    let crates_dir = repo_root.join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        return false;
    };
    let file_name = format!("{binary}.rs");
    for entry in entries.flatten() {
        let candidate = entry.path().join("tests").join(&file_name);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

/// Whether `s` is a canonical ISO `YYYY-MM-DD` calendar date: it parses as a real
/// date AND round-trips to the identical (zero-padded) string. Canonicality is what
/// makes the V6 lexicographic expiry comparison sound; a parseable-but-non-canonical
/// spelling (or a non-existent date like `2026-02-30`) is rejected.
fn is_canonical_date(s: &str) -> bool {
    s.parse::<jiff::civil::Date>()
        .is_ok_and(|d| d.to_string() == s)
}

/// Read the pinned version from a `verifiedAgainst` repo file. The only such file in
/// the seed is `fixtures/parity-corpus/oracle.json` (`{ "package", "version" }`), so
/// the machine-readable pin is its top-level string `"version"` (research/T009).
fn read_verified_pin(path: &Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("could not read file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("malformed JSON: {e}"))?;
    match value.get("version").and_then(|v| v.as_str()) {
        Some(version) => Ok(version.to_string()),
        None => Err("no top-level string `version` field".to_string()),
    }
}

fn record_type_name(ty: RecordType) -> &'static str {
    match ty {
        RecordType::Revision => "source-revision",
        RecordType::Source => "source-unit",
        RecordType::Dimension => "dimension",
        RecordType::Channel => "channel",
        RecordType::Profile => "profile",
        RecordType::Behavior => "behavior",
        RecordType::Case => "test-case",
        RecordType::Gap => "gap",
        RecordType::Waiver => "waiver",
        RecordType::Extension => "extension",
        RecordType::Constraint => "constraint-unit",
        RecordType::Classification => "classification",
        RecordType::ClauseUnit => "clause-unit",
        RecordType::ClauseClassification => "clause-classification",
    }
}

fn spec_name(spec: SpecStatus) -> &'static str {
    match spec {
        SpecStatus::Conformant => "conformant",
        SpecStatus::Nonconformant => "nonconformant",
        SpecStatus::Unspecified => "unspecified",
        SpecStatus::NotApplicable => "not-applicable",
    }
}

fn reference_name(reference: ReferenceStatus) -> &'static str {
    match reference {
        ReferenceStatus::Aligned => "aligned",
        ReferenceStatus::Divergent => "divergent",
        ReferenceStatus::Unknown => "unknown",
        ReferenceStatus::NotApplicable => "not-applicable",
    }
}

fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::FollowSpec => "follow-spec",
        Decision::AlignWithReference => "align-with-reference",
        Decision::DeaconExtension => "deacon-extension",
        Decision::IntentionalDivergence => "intentional-divergence",
        Decision::UnresolvedGap => "unresolved-gap",
    }
}

fn disposition_name(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::BehaviorMapped => "behavior-mapped",
        Disposition::NonTestable => "non-testable",
        Disposition::NotApplicable => "not-applicable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CertificationProfile, Condition, Decision, ReferenceStatus, SpecStatus, TestCase,
    };
    use indexmap::IndexMap;

    fn behavior(
        id: &str,
        spec: SpecStatus,
        reference: ReferenceStatus,
        decision: Decision,
        applicability: Vec<Condition>,
    ) -> BehaviorUnit {
        BehaviorUnit {
            id: id.to_string(),
            area: "test".to_string(),
            statement: "s".to_string(),
            applicability,
            spec,
            reference,
            decision,
            notes: None,
        }
    }

    fn cond(dimension: &str, values: &[&str]) -> Condition {
        Condition {
            dimension: dimension.to_string(),
            values: values.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn active_profile() -> CertificationProfile {
        let mut context = IndexMap::new();
        context.insert("dim-runtime".to_string(), "docker".to_string());
        CertificationProfile {
            id: "prof-x".to_string(),
            context,
            active: true,
        }
    }

    #[test]
    fn code_rank_orders_v10_after_v2() {
        assert!(code_rank("SCHEMA") < code_rank("V1"));
        assert!(code_rank("V2") < code_rank("V10"));
        assert!(code_rank("V9") < code_rank("V10"));
    }

    #[test]
    fn applies_in_profile_handles_empty_and_disjoint() {
        let profile = active_profile();
        let everywhere = behavior(
            "bhv-a",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        );
        assert!(applies_in_profile(&everywhere, &profile));

        let podman = behavior(
            "bhv-b",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![cond("dim-runtime", &["podman"])],
        );
        assert!(!applies_in_profile(&podman, &profile));

        let docker = behavior(
            "bhv-c",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![cond("dim-runtime", &["docker"])],
        );
        assert!(applies_in_profile(&docker, &profile));
    }

    #[test]
    fn conflicting_dimension_only_when_both_constrain_disjointly() {
        // Disjoint on a shared dimension → conflict.
        let a = vec![cond("dim-runtime", &["docker"])];
        let b = vec![cond("dim-runtime", &["podman"])];
        assert_eq!(
            conflicting_dimension(&a, &b).as_deref(),
            Some("dim-runtime")
        );

        // One side unconstrained on the dimension → no conflict.
        let empty: Vec<Condition> = vec![];
        assert!(conflicting_dimension(&a, &empty).is_none());

        // Overlapping values → no conflict.
        let c = vec![cond("dim-runtime", &["docker", "podman"])];
        assert!(conflicting_dimension(&a, &c).is_none());

        // Different dimensions → no conflict.
        let d = vec![cond("dim-os", &["linux"])];
        assert!(conflicting_dimension(&a, &d).is_none());
    }

    #[test]
    fn v2_flags_duplicate_and_prefix_mismatch() {
        let mut reg = Registry::default();
        // A behavior whose id prefix denotes a case → prefix↔type mismatch.
        reg.behaviors.push(behavior(
            "case-misprefixed",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        ));
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V2" && v.record == "case-misprefixed"),
            "expected a V2 prefix-mismatch, got {out:?}"
        );
    }

    #[test]
    fn v6_boundary_expires_equals_today_passes() {
        use crate::model::{Expect, Scope, Waiver};
        let waiver = Waiver {
            id: "wvr-x".to_string(),
            behaviors: vec![],
            scope: Scope::CorpusCase {
                corpus: "errors".to_string(),
                case: "malformed-json".to_string(),
            },
            expect: Expect::DeaconStricter { signal: None },
            rationale: "r".to_string(),
            added: "2026-01-01".to_string(),
            expires: "2026-07-19".to_string(),
            config: None,
        };
        let mut reg = Registry::default();
        reg.waivers.push(waiver);

        // expires == today → passes (no V6).
        let same = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(!same.iter().any(|v| v.code == "V6"), "boundary must pass");

        // expires < today → V6.
        let later = run(&reg, "2026-07-20", Path::new("/nonexistent-root"));
        assert!(
            later.iter().any(|v| v.code == "V6" && v.record == "wvr-x"),
            "expired waiver must be V6, got {later:?}"
        );
    }

    #[test]
    fn r8_exempts_extensions_but_flags_plain_behaviors() {
        let profile = active_profile();

        // A plain behavior in-profile, no case/waiver, reference not `unknown`
        // → R8 fires.
        let plain = behavior(
            "bhv-plain",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        );
        let mut reg = Registry::default();
        reg.profiles.push(profile.clone());
        reg.behaviors.push(plain);
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V8" && v.message.contains("R8")),
            "plain uncovered in-profile behavior must trip R8, got {out:?}"
        );

        // An extension behavior with reference `not-applicable`, same coverage
        // state → R8 exempt (no R8), even though V5 would still flag missing
        // coverage separately.
        let ext = behavior(
            "bhv-ext",
            SpecStatus::Unspecified,
            ReferenceStatus::NotApplicable,
            Decision::DeaconExtension,
            vec![],
        );
        let mut reg2 = Registry::default();
        reg2.profiles.push(profile);
        reg2.behaviors.push(ext);
        let out2 = run(&reg2, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            !out2
                .iter()
                .any(|v| v.code == "V8" && v.message.contains("R8")),
            "extension must be exempt from R8, got {out2:?}"
        );
    }

    #[test]
    fn v9_flags_undeclared_channel() {
        let mut reg = Registry::default();
        reg.cases.push(TestCase {
            id: "case-x".to_string(),
            behaviors: vec!["bhv-a".to_string()],
            context: vec![],
            executable: Some(crate::model::Executable {
                binary: "no_such_binary".to_string(),
                test: None,
                corpus: None,
                case: None,
            }),
            outcomes: vec![crate::model::ExpectedOutcome {
                channel: "chan-ghost".to_string(),
                expectation: "x".to_string(),
            }],
            ..TestCase::default()
        });
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter().any(|v| v.code == "V9" && v.record == "case-x"),
            "undeclared channel must be V9, got {out:?}"
        );
    }

    #[test]
    fn empty_registry_has_no_violations() {
        let reg = Registry::default();
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(out.is_empty(), "empty registry is valid, got {out:?}");
    }

    #[test]
    fn image_pinning_classifier() {
        assert!(image_is_pinned("alpine:3.19"));
        assert!(image_is_pinned(
            "mcr.microsoft.com/devcontainers/base:bookworm"
        ));
        assert!(image_is_pinned("foo@sha256:abcdef"));
        assert!(image_is_pinned("registry:5000/foo:1.2"));
        assert!(!image_is_pinned("alpine:latest"));
        assert!(!image_is_pinned("alpine"), "untagged is not pinned");
        assert!(
            !image_is_pinned("registry:5000/foo"),
            "a host-port colon is not a tag"
        );
    }

    #[test]
    fn v18_flags_unpinned_docker_fixture_image() {
        use crate::model::{ExpectedObservable, Operation, OracleType, ResourceGroup};
        // A temp fixtures tree with an unpinned (`latest`) image under conformance/fixtures.
        let repo = tempfile::tempdir().expect("tempdir");
        let fx = repo
            .path()
            .join("conformance/fixtures/fx-latest/.devcontainer");
        std::fs::create_dir_all(&fx).unwrap();
        std::fs::write(
            fx.join("devcontainer.json"),
            r#"{ "image": "alpine:latest" }"#,
        )
        .unwrap();

        let mut reg = Registry::default();
        reg.cases.push(TestCase {
            id: "case-docker-latest".to_string(),
            behaviors: vec!["bhv-a".to_string()],
            oracle_type: Some(OracleType::SpecExpectation),
            resource_group: Some(ResourceGroup::DockerShared),
            operations: vec![Operation {
                id: "op-up".to_string(),
                subcommand: "up".to_string(),
                fixtures: vec!["fx-latest".to_string()],
                ..Operation::default()
            }],
            expected: vec![ExpectedObservable {
                channel: "chan-exit-code".to_string(),
                operation: Some("op-up".to_string()),
                assertion: Some(serde_json::json!({ "equals": 0 })),
            }],
            ..TestCase::default()
        });
        let out = run(&reg, "2026-07-19", repo.path());
        assert!(
            out.iter()
                .any(|v| v.code == "V18" && v.record == "case-docker-latest"),
            "an unpinned Docker fixture image must be V18, got {out:?}"
        );

        // Re-pin to a concrete tag → no V18.
        std::fs::write(
            fx.join("devcontainer.json"),
            r#"{ "image": "alpine:3.19" }"#,
        )
        .unwrap();
        let out2 = run(&reg, "2026-07-19", repo.path());
        assert!(
            !out2.iter().any(|v| v.code == "V18"),
            "a pinned image must not trip V18, got {out2:?}"
        );
    }

    /// A well-formed declarative case, built directly (bypassing `Registry::load`), so
    /// the V16 checks in `run` are exercised on the correctly-shaped path.
    fn declarative_case(id: &str) -> TestCase {
        use crate::model::{ExpectedObservable, Operation, OracleType};
        TestCase {
            id: id.to_string(),
            behaviors: vec!["bhv-a".to_string()],
            operations: vec![Operation {
                id: "op-1".to_string(),
                subcommand: "read-configuration".to_string(),
                argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
                ..Operation::default()
            }],
            oracle_type: Some(OracleType::SpecExpectation),
            expected: vec![ExpectedObservable {
                channel: "chan-exit-code".to_string(),
                operation: Some("op-1".to_string()),
                assertion: Some(serde_json::json!({ "equals": 0 })),
            }],
            ..TestCase::default()
        }
    }

    /// A well-formed invariant-metamorphic case: two `up` ops, the second declaring an
    /// idempotence relationship against the first (a sibling).
    fn metamorphic_case(id: &str) -> TestCase {
        use crate::model::{Operation, OracleType, Relationship, RelationshipKind};
        let mut case = declarative_case(id);
        case.oracle_type = Some(OracleType::InvariantMetamorphic);
        case.operations = vec![
            Operation {
                id: "op-up-1".to_string(),
                subcommand: "up".to_string(),
                ..Operation::default()
            },
            Operation {
                id: "op-up-2".to_string(),
                subcommand: "up".to_string(),
                relationship: Some(Relationship {
                    kind: RelationshipKind::Idempotence,
                    against_op: "op-up-1".to_string(),
                }),
                ..Operation::default()
            },
        ];
        case
    }

    #[test]
    fn v20_accepts_well_formed_metamorphic_case() {
        let mut reg = Registry::default();
        reg.cases.push(metamorphic_case("case-meta-ok"));
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            !out.iter().any(|v| v.code == "V20"),
            "a well-formed metamorphic case must not trip V20, got {out:?}"
        );
    }

    #[test]
    fn v20_flags_metamorphic_arity_and_relationship_errors() {
        use crate::model::{Operation, Relationship, RelationshipKind};

        // < 2 ops.
        let mut one_op = metamorphic_case("case-meta-one-op");
        one_op.operations.truncate(1);
        one_op.operations[0].relationship = None;
        // no relationship.
        let mut no_rel = metamorphic_case("case-meta-no-rel");
        no_rel.operations[1].relationship = None;
        // relationship to a non-existent op.
        let mut dangling = metamorphic_case("case-meta-dangling");
        dangling.operations[1].relationship = Some(Relationship {
            kind: RelationshipKind::Idempotence,
            against_op: "op-nope".to_string(),
        });
        // relationship on a NON-metamorphic case.
        let mut spec_with_rel = declarative_case("case-spec-rel");
        spec_with_rel.operations.push(Operation {
            id: "op-2".to_string(),
            subcommand: "up".to_string(),
            relationship: Some(Relationship {
                kind: RelationshipKind::Idempotence,
                against_op: "op-1".to_string(),
            }),
            ..Operation::default()
        });

        for case in [one_op, no_rel, dangling, spec_with_rel] {
            let id = case.id.clone();
            let mut reg = Registry::default();
            reg.cases.push(case);
            let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
            assert!(
                out.iter().any(|v| v.code == "V20" && v.record == id),
                "{id} must trip V20, got {out:?}"
            );
        }
    }

    #[test]
    fn v16_accepts_a_well_formed_declarative_case() {
        let mut reg = Registry::default();
        reg.cases.push(declarative_case("case-decl-ok"));
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            !out.iter().any(|v| v.code == "V16"),
            "a well-formed declarative case must not trip V16, got {out:?}"
        );
    }

    #[test]
    fn v16_flags_non_consumer_subcommand() {
        let mut reg = Registry::default();
        let mut case = declarative_case("case-decl-bad-subcommand");
        case.operations[0].subcommand = "features".to_string(); // out of consumer scope
        reg.cases.push(case);
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V16" && v.record == "case-decl-bad-subcommand"),
            "a non-consumer subcommand must be V16 (Principle II), got {out:?}"
        );
    }

    #[test]
    fn v16_flags_mixed_and_neither_shapes() {
        let mut reg = Registry::default();
        // Mixed: both executable and operations.
        let mut mixed = declarative_case("case-mixed");
        mixed.executable = Some(crate::model::Executable {
            binary: "some_binary".to_string(),
            test: None,
            corpus: None,
            case: None,
        });
        reg.cases.push(mixed);
        // Neither: no executable, no operations.
        reg.cases.push(TestCase {
            id: "case-neither".to_string(),
            behaviors: vec!["bhv-a".to_string()],
            ..TestCase::default()
        });
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V16" && v.record == "case-mixed"),
            "a mixed legacy+declarative case must be V16, got {out:?}"
        );
        assert!(
            out.iter()
                .any(|v| v.code == "V16" && v.record == "case-neither"),
            "a shapeless case must be V16, got {out:?}"
        );
    }

    #[test]
    fn v16_enforces_fs_allowlist_iff_filesystem_channel() {
        // A filesystem-channel expectation with no allowlist trips V16.
        let mut reg = Registry::default();
        let mut needs_allowlist = declarative_case("case-fs-missing-allowlist");
        needs_allowlist
            .expected
            .push(crate::model::ExpectedObservable {
                channel: "chan-filesystem".to_string(),
                operation: Some("op-1".to_string()),
                assertion: Some(serde_json::json!({ "exists": "out.txt" })),
            });
        reg.cases.push(needs_allowlist);
        // An allowlist with no filesystem channel also trips V16 (forbidden otherwise).
        let mut stray_allowlist = declarative_case("case-fs-stray-allowlist");
        stray_allowlist.fs_allowlist = vec!["out.txt".to_string()];
        reg.cases.push(stray_allowlist);
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V16" && v.record == "case-fs-missing-allowlist"),
            "a filesystem expectation with no fsAllowlist must be V16, got {out:?}"
        );
        assert!(
            out.iter()
                .any(|v| v.code == "V16" && v.record == "case-fs-stray-allowlist"),
            "an fsAllowlist with no filesystem expectation must be V16, got {out:?}"
        );
    }

    #[test]
    fn v16_requires_assertions_for_spec_expectation() {
        let mut reg = Registry::default();
        let mut case = declarative_case("case-spec-no-assertion");
        case.expected[0].assertion = None; // spec-expectation needs it
        reg.cases.push(case);
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V16" && v.record == "case-spec-no-assertion"),
            "spec-expectation without an assertion must be V16, got {out:?}"
        );
    }
}
