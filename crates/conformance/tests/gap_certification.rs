//! Acceptance tests for gap-driven strict certification (T022; FR-020, FR-025,
//! SC-005) and the schema-constraint-inventory certification wiring (T037; SC-008,
//! contracts/cli-inventory.md).
//!
//! Drives the real `conformance` binary against two fixtures:
//!
//! - `valid` — carries one gap: it is structurally valid, so `report` succeeds and
//!   shows the gap, while `certify` exits 1 listing the gap as blocking;
//! - `gap-resolved` — the same registry with the gap resolved (case added,
//!   dispositions updated, gap record removed): `certify` exits 0. Its waiver does
//!   NOT block and appears under `waived`.
//!
//! The final block adds the T037 inventory-gate proofs: each of V11 (stale
//! classification), V12 (unclassified unit), V13 (malformed classification), and V14
//! (provenance break) BLOCKS `certify` (via the library [`certify`] over a small
//! hermetic fixture, mirroring `classification_join.rs`); a well-formed
//! `not-applicable`/`non-testable` classification NEVER blocks; and the REAL committed
//! registry certifies clean (exit 0) now that the gate is wired (SC-008).
//!
//! Uses `CARGO_BIN_EXE_conformance` and absolute fixture paths (via
//! `workspace_root()`), so it is CWD-independent and hermetic (no Docker/network).

use std::path::{Path, PathBuf};
use std::process::Command;

use deacon_conformance::certify::{BlockingKind, certify};
use deacon_conformance::inventory::{generate_inventory, write_inventory};
use deacon_conformance::load::Registry;
use deacon_conformance::model::ConstraintUnit;
use deacon_conformance::validate::{ClauseInputs, InventoryInputs};
use deacon_conformance::workspace_root;
use serde_json::{Value, json};
use tempfile::TempDir;

/// A fixed injected "today" so waiver-expiry (V6) never depends on the wall clock.
const TODAY: &str = "2026-07-19";

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("fixtures/conformance").join(name)
}

/// Run the `conformance` binary with the given subcommand args, returning
/// `(exit_code, stdout)`.
fn run(fixture_name: &str, args: &[&str]) -> (i32, String) {
    let bin = env!("CARGO_BIN_EXE_conformance");
    let output = Command::new(bin)
        .arg("--registry")
        .arg(fixture(fixture_name))
        .arg("--today")
        .arg(TODAY)
        .args(args)
        .output()
        .expect("conformance binary runs");
    let code = output.status.code().expect("process exited with a code");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    (code, stdout)
}

#[test]
fn gap_registry_certify_exits_1_listing_the_gap() {
    let (code, stdout) = run("valid", &["certify", "--json"]);
    assert_eq!(code, 1, "a registry with a gap must not certify (exit 1)");

    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("certify --json on stdout");
    assert_eq!(doc["certified"], false);
    let blocking = doc["blocking"].as_array().expect("blocking array");
    assert!(
        blocking
            .iter()
            .any(|b| b["kind"] == "gap" && b["id"] == "gap-readconfig-remote-user"),
        "the gap must be listed as blocking, got {blocking:?}"
    );
    // The waiver is enumerated but does NOT block certification (FR-025).
    let waived = doc["waived"].as_array().unwrap();
    assert!(
        waived.iter().any(|w| w == "wvr-readconfig-malformed-jsonc"),
        "the waiver must be enumerated under waived, got {waived:?}"
    );
}

#[test]
fn gap_registry_report_succeeds_and_shows_the_gap() {
    // `report` runs validation first; a gap is structurally valid, so it succeeds.
    let out_dir = tempfile::tempdir().expect("temp out-dir");
    let bin = env!("CARGO_BIN_EXE_conformance");
    let status = Command::new(bin)
        .arg("--registry")
        .arg(fixture("valid"))
        .arg("--today")
        .arg(TODAY)
        .arg("report")
        .arg("--out-dir")
        .arg(out_dir.path())
        .status()
        .expect("report runs");
    assert_eq!(
        status.code(),
        Some(0),
        "report on a valid+gapped registry succeeds"
    );

    let json = std::fs::read_to_string(out_dir.path().join("report.json")).expect("report.json");
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        report["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"] == "gap-readconfig-remote-user"),
        "report.json must surface the gap (FR-020)"
    );
    assert_eq!(report["summary"]["gap"], 1, "the gap counts in the summary");

    // report.md always has a Gaps section that names the gap.
    let md = std::fs::read_to_string(out_dir.path().join("report.md")).expect("report.md");
    assert!(md.contains("## Gaps"), "report.md must have a Gaps section");
    assert!(
        md.contains("gap-readconfig-remote-user"),
        "report.md Gaps section must name the gap"
    );
}

#[test]
fn resolved_gap_registry_certifies_and_waiver_is_non_blocking() {
    let (code, stdout) = run("gap-resolved", &["certify", "--json"]);
    assert_eq!(
        code, 0,
        "with the gap resolved, the registry certifies (exit 0)"
    );

    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["certified"], true);
    assert!(
        doc["blocking"].as_array().unwrap().is_empty(),
        "a resolved registry has no blocking items"
    );
    // The waiver-covered behavior does not block, but the waiver still appears.
    let waived = doc["waived"].as_array().unwrap();
    assert!(
        waived.iter().any(|w| w == "wvr-readconfig-malformed-jsonc"),
        "the waiver must still be enumerated under waived (non-blocking), got {waived:?}"
    );
}

#[test]
fn resolved_gap_report_marks_the_behavior_covered_not_gap() {
    let registry = deacon_conformance::load::Registry::load(&fixture("gap-resolved"))
        .expect("gap-resolved loads");
    let json = deacon_conformance::report::render_report_json(&registry);
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();

    // No gaps remain, and the once-gapped behavior is now a covered (non-gap) entry.
    assert_eq!(report["summary"]["gap"], 0, "no gaps after resolution");
    assert!(
        report["gaps"].as_array().unwrap().is_empty(),
        "gaps array is empty after resolution"
    );
    let resolved = report["behaviors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "bhv-readconfig-remote-user-probe")
        .expect("the resolved behavior is now in-profile covered");
    assert_ne!(
        resolved["coverage"], "gap",
        "the resolved behavior must no longer report as a gap"
    );
}

// ===========================================================================
// T037 — the schema-constraint-inventory join (V11–V14) blocks certify.
//
// Each defect is injected into a small self-consistent fixture (temp schemas +
// in-place-generated committed inventory + temp registry), then the library
// `certify` gate is evaluated. The fixture-building mirrors
// `classification_join.rs`; here we assert the *certification verdict* (a
// `constraint` blocker of the expected V-class), not the raw violation list.
// ===========================================================================

/// A tiny object schema yielding a handful of facet units (as in `classification_join`).
const CST_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "count": { "type": "integer" }
  },
  "additionalProperties": false
}"#;

/// The fixture's schema-revision id (the inventory's `revision`, pinned in the registry).
const CST_REVISION: &str = "rev-schema-fixture";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// A self-consistent schemas + committed-inventory pair (fingerprint verified,
/// committed == regeneration, revision pinned to [`CST_REVISION`]) so that, absent an
/// injected defect, the V14 provenance checks are clean.
struct InventoryFixture {
    _tmp: TempDir,
    schemas_dir: PathBuf,
    inventory_file: PathBuf,
    units: Vec<ConstraintUnit>,
}

impl InventoryFixture {
    fn build() -> InventoryFixture {
        let tmp = tempfile::tempdir().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        write_file(&schemas_dir.join("schema.json"), CST_SCHEMA);
        let sha = sha256_hex(CST_SCHEMA.as_bytes());
        let manifest = format!(
            r#"{{ "schemaVersion": 1, "revision": "{CST_REVISION}", "documents": [
                {{ "key": "fixture", "file": "schema.json",
                   "upstreamUrl": "https://example.invalid/s.json", "sha256": "{sha}" }}
            ] }}"#
        );
        write_file(&schemas_dir.join("manifest.json"), &manifest);

        let inventory = generate_inventory(&schemas_dir).expect("fixture schema extracts");
        assert!(inventory.units.len() >= 2, "fixture must yield >=2 units");
        let inventory_file = tmp.path().join("inventory/constraints.json");
        write_inventory(&inventory_file, &inventory).expect("committed inventory writes");

        InventoryFixture {
            _tmp: tmp,
            schemas_dir,
            inventory_file,
            units: inventory.units,
        }
    }

    fn inputs(&self) -> InventoryInputs<'_> {
        InventoryInputs {
            schemas_dir: &self.schemas_dir,
            inventory_file: &self.inventory_file,
        }
    }
}

/// The `cls-` id that correctly mirrors a `cst-` constraint id.
fn cls_id(constraint: &str) -> String {
    format!("cls-{}", constraint.strip_prefix("cst-").expect("cst- id"))
}

/// A valid `non-testable` classification for a unit (mirrored id, rationale, no behaviors).
fn valid_cls(unit: &ConstraintUnit) -> Value {
    json!({
        "id": cls_id(&unit.id),
        "constraint": unit.id,
        "disposition": "non-testable",
        "rationale": "fixture rationale"
    })
}

/// Valid classifications for every unit EXCEPT the named constraint.
fn valid_except(units: &[ConstraintUnit], except: &str) -> Vec<Value> {
    units
        .iter()
        .filter(|u| u.id != except)
        .map(valid_cls)
        .collect()
}

/// Build a temp registry directory with a `schema`-kind revision named `revision`,
/// the given `behaviors` (`follow-spec`/`conformant`), and `classifications`.
fn inventory_registry(
    revision: &str,
    behaviors: &[&str],
    classifications: &[Value],
) -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    write_file(
        &dir.join("revisions.json"),
        &format!(
            r#"{{ "schemaVersion": 1, "records": [
                {{ "id": "{revision}", "kind": "schema", "pin": "fixture",
                   "url": "https://example.invalid" }}
            ] }}"#
        ),
    );
    if !behaviors.is_empty() {
        let records: Vec<Value> = behaviors
            .iter()
            .map(|b| {
                json!({
                    "id": b, "area": "fixture", "statement": "s",
                    "spec": "conformant", "reference": "aligned", "decision": "follow-spec"
                })
            })
            .collect();
        write_file(
            &dir.join("behaviors/fixture.json"),
            &serde_json::to_string_pretty(&json!({ "schemaVersion": 1, "records": records }))
                .unwrap(),
        );
    }
    write_file(
        &dir.join("classifications/fixture.json"),
        &serde_json::to_string_pretty(&json!({ "schemaVersion": 1, "records": classifications }))
            .unwrap(),
    );
    (tmp, dir)
}

/// Certify a temp registry against a fixture inventory (the library gate).
fn certify_fixture(
    fx: &InventoryFixture,
    registry_dir: &Path,
) -> deacon_conformance::certify::Certification {
    let reg = Registry::load(registry_dir).expect("fixture registry loads");
    certify(&reg, &fx.inputs(), &no_clause_inputs())
}

/// Clause inputs pointing at absent paths, so the clause join scopes itself out (these
/// constraint-inventory fixtures ship no committed clause inventory / vendored prose).
fn no_clause_inputs() -> ClauseInputs<'static> {
    ClauseInputs {
        spec_dir: Path::new("/nonexistent-conformance/spec"),
        clauses_file: Path::new("/nonexistent-conformance/inventory/clauses.json"),
    }
}

/// Assert a `constraint` blocker of `code` naming `record` is present.
fn has_constraint_blocker(
    cert: &deacon_conformance::certify::Certification,
    code: &str,
    record: &str,
) -> bool {
    cert.blocking.iter().any(|b| {
        b.kind == BlockingKind::Constraint && b.code.as_deref() == Some(code) && b.id == record
    })
}

#[test]
fn v11_stale_classification_blocks_certify() {
    let fx = InventoryFixture::build();
    let ghost = "cst-fixture-ghost-type-00000000";
    let mut classifications: Vec<Value> = fx.units.iter().map(valid_cls).collect();
    classifications.push(json!({
        "id": cls_id(ghost), "constraint": ghost,
        "disposition": "non-testable", "rationale": "points at a removed unit"
    }));
    let (_reg, dir) = inventory_registry(CST_REVISION, &[], &classifications);

    let cert = certify_fixture(&fx, &dir);
    assert!(
        !cert.certified,
        "a stale classification (V11) must block certify"
    );
    assert!(
        has_constraint_blocker(&cert, "V11", &cls_id(ghost)),
        "V11 must appear as a constraint blocker naming the stale record, got: {:#?}",
        cert.blocking
    );
}

#[test]
fn v12_unclassified_unit_blocks_certify() {
    let fx = InventoryFixture::build();
    let target = fx.units[0].id.clone();
    // Classify every unit EXCEPT the first → the first is unclassified.
    let classifications = valid_except(&fx.units, &target);
    let (_reg, dir) = inventory_registry(CST_REVISION, &[], &classifications);

    let cert = certify_fixture(&fx, &dir);
    assert!(
        !cert.certified,
        "an unclassified unit (V12) must block certify"
    );
    assert!(
        has_constraint_blocker(&cert, "V12", &target),
        "V12 must appear as a constraint blocker naming the unclassified unit, got: {:#?}",
        cert.blocking
    );
}

#[test]
fn v13_malformed_classification_blocks_certify() {
    let fx = InventoryFixture::build();
    let target = fx.units[0].id.clone();
    // `behavior-mapped` with an empty `behaviors` list — a V13 arity violation.
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": cls_id(&target), "constraint": target,
        "disposition": "behavior-mapped", "behaviors": []
    }));
    let (_reg, dir) = inventory_registry(CST_REVISION, &[], &classifications);

    let cert = certify_fixture(&fx, &dir);
    assert!(
        !cert.certified,
        "a malformed classification (V13) must block certify"
    );
    assert!(
        has_constraint_blocker(&cert, "V13", &cls_id(&fx.units[0].id)),
        "V13 must appear as a constraint blocker naming the malformed record, got: {:#?}",
        cert.blocking
    );
}

#[test]
fn v14_provenance_break_blocks_certify() {
    let fx = InventoryFixture::build();
    // Tamper the vendored schema WITHOUT updating the manifest sha256 → V14 fingerprint.
    let tampered = CST_SCHEMA.replacen("\"string\"", "\"number\"", 1);
    assert_ne!(tampered, CST_SCHEMA, "tamper must change the bytes");
    write_file(&fx.schemas_dir.join("schema.json"), &tampered);

    let classifications: Vec<Value> = fx.units.iter().map(valid_cls).collect();
    let (_reg, dir) = inventory_registry(CST_REVISION, &[], &classifications);

    let cert = certify_fixture(&fx, &dir);
    assert!(
        !cert.certified,
        "a provenance break (V14) must block certify"
    );
    assert!(
        cert.blocking
            .iter()
            .any(|b| b.kind == BlockingKind::Constraint && b.code.as_deref() == Some("V14")),
        "V14 must appear as a constraint blocker, got: {:#?}",
        cert.blocking
    );
}

#[test]
fn well_formed_not_applicable_and_non_testable_never_block_certify() {
    // Every unit classified with a well-formed non-blocking disposition — the honest
    // consumer-only-scope boundary — must NOT produce any constraint blocker.
    let fx = InventoryFixture::build();
    let mut classifications: Vec<Value> = Vec::new();
    for (i, unit) in fx.units.iter().enumerate() {
        let disposition = if i % 2 == 0 {
            "not-applicable"
        } else {
            "non-testable"
        };
        classifications.push(json!({
            "id": cls_id(&unit.id), "constraint": unit.id,
            "disposition": disposition, "rationale": "outside consumer scope / no testable behavior"
        }));
    }
    let (_reg, dir) = inventory_registry(CST_REVISION, &[], &classifications);

    let cert = certify_fixture(&fx, &dir);
    assert!(
        cert.certified,
        "a fully not-applicable/non-testable fixture must certify (no blockers), got: {:#?}",
        cert.blocking
    );
    assert!(
        !cert
            .blocking
            .iter()
            .any(|b| b.kind == BlockingKind::Constraint),
        "not-applicable/non-testable must never be a constraint blocker, got: {:#?}",
        cert.blocking
    );
}

#[test]
fn real_registry_certifies_clean_now_that_the_gate_is_wired() {
    // SC-008: the real, fully-classified committed registry certifies with exit 0 even
    // though `certify` now enforces V11–V14. Drives the actual binary with NO
    // `--registry` override, so it uses the real `conformance/registry` + its sibling
    // committed inventory + vendored schemas (the true release-gate configuration).
    let bin = env!("CARGO_BIN_EXE_conformance");
    let output = Command::new(bin)
        .arg("--today")
        .arg(TODAY)
        .arg("certify")
        .arg("--json")
        .output()
        .expect("conformance binary runs against the real registry");
    let code = output.status.code().expect("process exited with a code");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert_eq!(
        code,
        0,
        "the real registry must certify clean now that V11-V14 gate certify; stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("certify --json on stdout");
    assert_eq!(doc["certified"], true, "real registry must be certified");
    assert!(
        doc["blocking"]
            .as_array()
            .expect("blocking array")
            .is_empty(),
        "the real registry has zero blocking items, got: {:?}",
        doc["blocking"]
    );
}

// ===========================================================================
// Normative-clause-inventory certification wiring (021, T033; FR-018/FR-022,
// SC-008). Proves an unclassified/stale clause blocks `certify`, and well-formed
// not-applicable/non-testable clause dispositions never do — over the fixture prose
// (`fixtures/conformance/prose`, seven committed clauses) via the library `certify`.
// ===========================================================================

use deacon_conformance::model::{ClauseClassification, Disposition, RevisionKind, SourceRevision};

fn prose_spec_dir() -> PathBuf {
    workspace_root().join("fixtures/conformance/prose")
}

/// The canonical ids of the committed fixture clauses.
fn fixture_clause_ids() -> Vec<String> {
    deacon_conformance::clause::generate_clauses(
        &prose_spec_dir(),
        &prose_spec_dir().join("clauses.json"),
    )
    .expect("fixture clauses canonicalize")
    .units
    .into_iter()
    .map(|u| u.id)
    .collect()
}

/// A registry carrying only a `spec`-kind revision (so V14's revision-pin check passes)
/// plus the given clause classifications.
fn clause_registry(classifications: Vec<ClauseClassification>) -> Registry {
    Registry {
        revisions: vec![SourceRevision {
            id: "rev-spec-113500f4".to_string(),
            kind: RevisionKind::Spec,
            pin: "113500f4".to_string(),
            url: "u".to_string(),
            verified_against: None,
        }],
        clause_classifications: classifications,
        ..Default::default()
    }
}

fn certify_clauses(registry: &Registry) -> deacon_conformance::certify::Certification {
    let spec = prose_spec_dir();
    let clauses = spec.join("clauses.json");
    // No constraint inventory here — scope that join out; exercise ONLY the clause gate.
    certify(
        registry,
        &InventoryInputs {
            schemas_dir: Path::new("/nonexistent-conformance/schemas"),
            inventory_file: Path::new("/nonexistent-conformance/inventory/constraints.json"),
        },
        &ClauseInputs {
            spec_dir: &spec,
            clauses_file: &clauses,
        },
    )
}

#[test]
fn unclassified_clause_blocks_certify() {
    // No clause classifications at all → every fixture clause is unclassified (V12).
    let cert = certify_clauses(&clause_registry(vec![]));
    assert!(!cert.certified, "unclassified clauses must block certify");
    assert!(
        cert.blocking
            .iter()
            .any(|b| b.kind == BlockingKind::Clause && b.code.as_deref() == Some("V12")),
        "an unclassified clause must appear as a Clause V12 blocker, got: {:#?}",
        cert.blocking
    );
}

#[test]
fn not_applicable_and_non_testable_clause_dispositions_never_block() {
    // Classify every fixture clause with a well-formed non-blocking disposition.
    let classifications: Vec<ClauseClassification> = fixture_clause_ids()
        .into_iter()
        .enumerate()
        .map(|(i, clause)| {
            let tail = clause.strip_prefix("clu-").unwrap().to_string();
            ClauseClassification {
                id: format!("clc-{tail}"),
                clause: Some(clause),
                document: None,
                disposition: if i % 2 == 0 {
                    Disposition::NotApplicable
                } else {
                    Disposition::NonTestable
                },
                behaviors: vec![],
                rationale: Some(
                    "outside consumer runtime scope / no testable behavior".to_string(),
                ),
                notes: None,
            }
        })
        .collect();
    let cert = certify_clauses(&clause_registry(classifications));
    assert!(
        !cert.blocking.iter().any(|b| b.kind == BlockingKind::Clause),
        "well-formed not-applicable/non-testable clauses must never block, got: {:#?}",
        cert.blocking
    );
    assert!(cert.certified, "a fully-classified fixture must certify");
}

#[test]
fn stale_clause_classification_blocks_certify() {
    // A classification pointing at a clause that does not exist → V11 stale.
    let ghost = "clu-sample-ghost-must-00000000";
    let mut classifications: Vec<ClauseClassification> = fixture_clause_ids()
        .into_iter()
        .map(|clause| {
            let tail = clause.strip_prefix("clu-").unwrap().to_string();
            ClauseClassification {
                id: format!("clc-{tail}"),
                clause: Some(clause),
                document: None,
                disposition: Disposition::NotApplicable,
                behaviors: vec![],
                rationale: Some("r".to_string()),
                notes: None,
            }
        })
        .collect();
    classifications.push(ClauseClassification {
        id: format!("clc-{}", ghost.strip_prefix("clu-").unwrap()),
        clause: Some(ghost.to_string()),
        document: None,
        disposition: Disposition::NotApplicable,
        behaviors: vec![],
        rationale: Some("points at a removed clause".to_string()),
        notes: None,
    });
    let cert = certify_clauses(&clause_registry(classifications));
    assert!(
        cert.blocking
            .iter()
            .any(|b| b.kind == BlockingKind::Clause && b.code.as_deref() == Some("V11")),
        "a stale clause classification must block as Clause V11, got: {:#?}",
        cert.blocking
    );
}
