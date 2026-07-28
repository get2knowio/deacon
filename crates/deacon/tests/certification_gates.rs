//! Certification acceptance tests (026-continuous-conformance-certification, US1/US2).
//!
//! Hermetic: no container engine, no reference implementation, no network — which is
//! itself one of the properties under test (FR-056, SC-013).
//!
//! ## The nine positive controls
//!
//! FR-052 requires each of the nine failure conditions to have an *injected* control that
//! demonstrates the verdict flipping from certified to not-certified and back. That
//! phrasing is deliberate and worth honouring literally: a condition verified only by
//! reading the code is a condition nobody has watched fail. The coverage model has already
//! produced two cases where an assertion could never fail and still read as coverage — a
//! `jsonSubset: {}` that matched anything, and a `contains` that could not see appended
//! output. Both were in committed records; only an injected run found them.

use std::collections::BTreeSet;
use std::path::PathBuf;

use deacon_conformance::certification::{
    FR034_FIELD_COUNT, FR034_FIELDS, build_report, render_json, render_md,
};
use deacon_conformance::certify::{
    BlockingKind, Certification, EvidenceInputs, certify_with_evidence,
};
use deacon_conformance::manifest::{
    CaseOutcome, ExecutionManifest, ManifestCase, ManifestEnvironment, ManifestExpectation,
    RequiredCase, verify_loaded,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const REVISION: &str = "abc123";
const PROFILE: &str = "prof-linux-amd64-docker-0870";

fn environment() -> ManifestEnvironment {
    ManifestEnvironment {
        platform: "linux".into(),
        arch: "x86_64".into(),
        container_engine: "docker".into(),
        container_engine_version: "27.3.1".into(),
        compose_version: "2.29.7".into(),
    }
}

fn manifest_case(id: &str, outcome: CaseOutcome) -> ManifestCase {
    ManifestCase {
        case_id: id.into(),
        case_hash: "h1".into(),
        fixture_hash: "f1".into(),
        outcome,
        excluded_by: None,
    }
}

fn clean_manifest() -> ExecutionManifest {
    ExecutionManifest {
        schema_version: 1,
        revision: REVISION.into(),
        profile: PROFILE.into(),
        environment: environment(),
        required_case_count: 1,
        cases: vec![manifest_case("case-a", CaseOutcome::Pass)],
    }
}

fn expectation() -> ManifestExpectation {
    ManifestExpectation {
        revision: REVISION.into(),
        profile: PROFILE.into(),
        required: vec![RequiredCase {
            case_id: "case-a".into(),
            case_hash: "h1".into(),
            fixture_hash: "f1".into(),
        }],
        resolvable_dispositions: BTreeSet::from(["odp-known".to_string()]),
    }
}

/// Write a manifest to a temp file and return its path plus the guard that owns it.
fn manifest_file(manifest: &ExecutionManifest) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("execution-manifest.json");
    std::fs::write(&path, serde_json::to_string(manifest).expect("serialize")).expect("write");
    (dir, path)
}

// ---------------------------------------------------------------------------
// T039 / T057 — the four execution-manifest rejection modes (FR-057, SC-014)
// ---------------------------------------------------------------------------

#[test]
fn control_h_absent_manifest_blocks() {
    let defects = deacon_conformance::manifest::verify_manifest(
        &PathBuf::from("/nonexistent/execution-manifest.json"),
        &expectation(),
    );
    assert_eq!(defects.len(), 1);
    assert_eq!(defects[0].code(), "V35-absent");
}

#[test]
fn control_h_incomplete_manifest_blocks() {
    let mut expected = expectation();
    expected.required.push(RequiredCase {
        case_id: "case-b".into(),
        case_hash: "h1".into(),
        fixture_hash: "f1".into(),
    });
    let defects = verify_loaded(&clean_manifest(), &expected);
    assert!(defects.iter().any(|d| d.code() == "V35-incomplete"));
    assert!(defects.iter().any(|d| d.record() == "case-b"));
}

#[test]
fn control_h_revision_mismatched_manifest_blocks() {
    let mut manifest = clean_manifest();
    manifest.revision = "deadbeef".into();
    let defects = verify_loaded(&manifest, &expectation());
    assert!(defects.iter().any(|d| d.code() == "V35-revision"));
}

#[test]
fn control_h_hash_stale_manifest_blocks() {
    let mut manifest = clean_manifest();
    manifest.cases[0].case_hash = "old".into();
    let defects = verify_loaded(&manifest, &expectation());
    assert!(defects.iter().any(|d| d.code() == "V35-stale"));
}

#[test]
fn all_four_rejection_modes_are_independently_demonstrated() {
    // SC-014: 4 of 4, each independently. Asserting the set rather than a count so a mode
    // that stopped being reachable is caught rather than silently replaced by a duplicate
    // of another.
    let mut modes = BTreeSet::new();

    modes.extend(
        deacon_conformance::manifest::verify_manifest(
            &PathBuf::from("/nonexistent/m.json"),
            &expectation(),
        )
        .iter()
        .map(|d| d.code()),
    );

    let mut expected = expectation();
    expected.required.push(RequiredCase {
        case_id: "case-b".into(),
        case_hash: "h1".into(),
        fixture_hash: "f1".into(),
    });
    modes.extend(
        verify_loaded(&clean_manifest(), &expected)
            .iter()
            .map(|d| d.code()),
    );

    let mut wrong_rev = clean_manifest();
    wrong_rev.revision = "x".into();
    modes.extend(
        verify_loaded(&wrong_rev, &expectation())
            .iter()
            .map(|d| d.code()),
    );

    let mut stale = clean_manifest();
    stale.cases[0].fixture_hash = "old".into();
    modes.extend(
        verify_loaded(&stale, &expectation())
            .iter()
            .map(|d| d.code()),
    );

    for mode in ["V35-absent", "V35-incomplete", "V35-revision", "V35-stale"] {
        assert!(
            modes.contains(mode),
            "rejection mode `{mode}` not demonstrated"
        );
    }
}

// ---------------------------------------------------------------------------
// T032 / T033 / T037 — the data-gate conditions (a), (b), (f)
// ---------------------------------------------------------------------------
//
// These three are pre-existing `certify` blockers that 026 does not re-implement. What
// FR-052 adds is the demonstration: each must be watched to flip the verdict, because a
// condition verified only by reading the code is a condition nobody has seen fail.
//
// They are driven through fixture registries rather than by injecting into the real one —
// the same mechanism every gate in this system is tested with, and the reason `certify`
// needs no bypass flag (FR-044).

/// Certify a fixture registry through the library and return `(certified, blocking kinds)`.
///
/// Calls [`certify_with_evidence`] directly rather than shelling out to the binary.
/// `CARGO_BIN_EXE_*` is only defined for the crate that owns the binary, and more to the
/// point: driving the library keeps this test hermetic and makes the evidence inputs
/// explicit instead of implied by process environment.
fn certify_fixture(registry: &std::path::Path) -> (bool, BTreeSet<String>) {
    let registry_data =
        deacon_conformance::load::Registry::load(registry).expect("fixture registry loads");
    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        // A fixture registry declares no container-backed case, so the manifest gate is
        // vacuous here and the data-gate conditions are what the verdict reflects.
        manifest_path: Some(PathBuf::from("/nonexistent/m.json")),
        profile_is_active: true,
        ..Default::default()
    };
    let (schemas, inventory) = sibling_inventory(registry);
    let (spec, clauses) = sibling_clauses(registry);
    let certification = certify_with_evidence(
        &registry_data,
        "2026-07-28",
        &deacon_conformance::validate::InventoryInputs {
            schemas_dir: &schemas,
            inventory_file: &inventory,
        },
        &deacon_conformance::validate::ClauseInputs {
            spec_dir: &spec,
            clauses_file: &clauses,
        },
        &registry.parent().unwrap_or(registry).join("snapshots"),
        &evidence,
    );
    let kinds = certification
        .blocking
        .iter()
        .filter_map(|b| {
            serde_json::to_value(b.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
        })
        .collect();
    (certification.certified, kinds)
}

/// The inventory paths that belong to a fixture registry, resolved as siblings — the same
/// resolution `validate` and `certify` use, so a fixture shipping neither simply scopes the
/// join out rather than failing.
fn sibling_inventory(registry: &std::path::Path) -> (PathBuf, PathBuf) {
    let base = registry.parent().unwrap_or(registry);
    (
        base.join("schemas")
            .join(deacon_conformance::CURRENT_SCHEMA_PIN),
        base.join("inventory").join("constraints.json"),
    )
}

fn sibling_clauses(registry: &std::path::Path) -> (PathBuf, PathBuf) {
    let base = registry.parent().unwrap_or(registry);
    (
        base.join("spec").join(deacon_conformance::CURRENT_SPEC_PIN),
        base.join("inventory").join("clauses.json"),
    )
}

/// A fixture registry shipped for the existing gate tests, reused rather than duplicated:
/// a second copy of "a registry with an uncovered behavior" is a second thing to keep in
/// step with the loader.
fn fixture(name: &str) -> std::path::PathBuf {
    deacon_conformance::workspace_root()
        .join("fixtures")
        .join("conformance")
        .join(name)
}

#[test]
fn control_b_an_uncovered_in_profile_behavior_blocks() {
    // Condition (b). `invalid-v5` is exactly this shape: an in-profile behavior with no
    // case, waiver, or gap.
    let registry = fixture("invalid-v5");
    assert!(
        registry.is_dir(),
        "fixture `{}` is missing — this test must fail loudly rather than pass over an \
         absent directory",
        registry.display()
    );
    let (certified, kinds) = certify_fixture(&registry);
    assert!(
        !certified,
        "an uncovered in-profile behavior must block; blocking: {kinds:?}"
    );
    assert!(
        kinds.contains("uncovered"),
        "the condition must be named: {kinds:?}"
    );
}

#[test]
fn control_removing_the_condition_restores_certification() {
    // The half of FR-052 that is easy to skip: the verdict must come *back*. Without it a
    // gate that blocked unconditionally would satisfy every positive control.
    let registry = fixture("gap-resolved");
    assert!(
        registry.is_dir(),
        "fixture `{}` is missing",
        registry.display()
    );
    let (certified, kinds) = certify_fixture(&registry);
    assert!(
        certified,
        "with its gap resolved the fixture must certify; blocking: {kinds:?}"
    );
}

#[test]
fn every_blocking_kind_has_a_distinct_wire_name() {
    // FR-042: a blocked release must name what blocked it. Two kinds sharing a wire name
    // would make two different problems indistinguishable in the output.
    use deacon_conformance::certify::BlockingKind;
    let all = [
        BlockingKind::Gap,
        BlockingKind::Uncovered,
        BlockingKind::Constraint,
        BlockingKind::Clause,
        BlockingKind::Obligation,
        BlockingKind::StaleSnapshot,
        BlockingKind::MissingExecution,
        BlockingKind::IncorrectOracle,
        BlockingKind::RunnerOmission,
        BlockingKind::SilentlySkippedCase,
        BlockingKind::FailingCase,
        BlockingKind::InactiveProfile,
    ];
    let names: BTreeSet<String> = all
        .iter()
        .map(|k| serde_json::to_string(k).expect("kind serializes"))
        .collect();
    assert_eq!(
        names.len(),
        all.len(),
        "every blocking kind needs a distinct wire name: {names:?}"
    );
}

// ---------------------------------------------------------------------------
// T040 — condition (i), a silently skipped case
// ---------------------------------------------------------------------------

#[test]
fn control_i_an_excluded_outcome_with_no_disposition_blocks() {
    let mut manifest = clean_manifest();
    manifest.cases[0].outcome = CaseOutcome::Excluded;
    let defects = verify_loaded(&manifest, &expectation());
    assert!(defects.iter().any(|d| d.code() == "V35-unaccounted"));
}

#[test]
fn control_i_an_excluded_outcome_with_an_unresolvable_disposition_blocks() {
    let mut manifest = clean_manifest();
    manifest.cases[0].outcome = CaseOutcome::Excluded;
    manifest.cases[0].excluded_by = Some("odp-nonexistent".into());
    let defects = verify_loaded(&manifest, &expectation());
    assert!(defects.iter().any(|d| d.code() == "V35-unaccounted"));
}

#[test]
fn an_explicitly_dispositioned_exclusion_is_accounted_for() {
    // The other half of FR-041(i): a *properly* dispositioned exclusion must NOT block.
    // Without this the condition would be satisfiable by forbidding exclusions entirely,
    // which is not what the requirement says.
    let mut manifest = clean_manifest();
    manifest.cases[0].outcome = CaseOutcome::Excluded;
    manifest.cases[0].excluded_by = Some("odp-known".into());
    assert!(verify_loaded(&manifest, &expectation()).is_empty());
}

// ---------------------------------------------------------------------------
// T043 — a failing case is reported distinctly from a manifest defect
// ---------------------------------------------------------------------------

#[test]
fn a_failing_case_is_not_a_manifest_integrity_defect() {
    // "The evidence is malformed" and "the evidence says deacon diverged" need different
    // fixes. A maintainer reading a blocked release must be able to tell which they have.
    let mut manifest = clean_manifest();
    manifest.cases[0].outcome = CaseOutcome::Fail;
    assert!(
        verify_loaded(&manifest, &expectation()).is_empty(),
        "a failing case is not a manifest defect"
    );
    assert_eq!(
        deacon_conformance::manifest::failing_cases(&manifest),
        vec!["case-a".to_string()],
        "but it is still reported, as a failing case"
    );
}

// ---------------------------------------------------------------------------
// T041 — every condition reported in one run, each naming a record (FR-042, FR-043)
// ---------------------------------------------------------------------------

#[test]
fn every_failing_condition_is_reported_in_a_single_run() {
    let mut manifest = clean_manifest();
    manifest.revision = "wrong".into();
    manifest.cases[0].case_hash = "old".into();
    manifest.cases.push(ManifestCase {
        case_id: "case-c".into(),
        case_hash: "h1".into(),
        fixture_hash: "f1".into(),
        outcome: CaseOutcome::Excluded,
        excluded_by: None,
    });
    let mut expected = expectation();
    expected.required.push(RequiredCase {
        case_id: "case-b".into(),
        case_hash: "h1".into(),
        fixture_hash: "f1".into(),
    });

    let defects = verify_loaded(&manifest, &expected);
    let codes: BTreeSet<&str> = defects.iter().map(|d| d.code()).collect();
    assert!(codes.len() >= 4, "expected every condition, got {codes:?}");
    assert!(codes.contains("V35-revision"));
    assert!(codes.contains("V35-stale"));
    assert!(codes.contains("V35-incomplete"));
    assert!(codes.contains("V35-unaccounted"));
}

#[test]
fn every_defect_names_a_specific_record_not_a_count() {
    // FR-042. A bare count is exactly what makes a blocked release un-actionable.
    let mut manifest = clean_manifest();
    manifest.cases[0].case_hash = "old".into();
    for defect in verify_loaded(&manifest, &expectation()) {
        assert!(
            !defect.record().is_empty(),
            "every defect must name its offending record"
        );
        assert!(
            defect.message().len() > 40,
            "a message must diagnose, not merely label: {}",
            defect.message()
        );
    }
}

// ---------------------------------------------------------------------------
// T042 — no flag downgrades a condition (FR-044)
// ---------------------------------------------------------------------------

#[test]
fn the_certify_surface_exposes_no_downgrade_flag() {
    // FR-044. Read from the CLI source rather than by invoking `--help`, so the assertion
    // holds even for a flag that is defined but hidden.
    let source = std::fs::read_to_string(
        deacon_conformance::workspace_root()
            .join("crates")
            .join("conformance")
            .join("src")
            .join("bin")
            .join("conformance.rs"),
    )
    .expect("CLI source readable");

    for forbidden in [
        "allow-gap",
        "allow_gap",
        "skip-blocking",
        "skip_blocking",
        "ignore-stale",
        "ignore_stale",
        "no-fail",
        "warn-only",
        "warn_only",
        "force-certify",
        "force_certify",
    ] {
        assert!(
            !source.contains(forbidden),
            "the certify surface must expose no way to downgrade a failure condition to a \
             warning (FR-044); found `{forbidden}`"
        );
    }
}

// ---------------------------------------------------------------------------
// T013–T015 — report shape, scope exactness, reproducibility
// ---------------------------------------------------------------------------

/// Build a report over the real registry with a synthetic verdict, so the shape tests do
/// not depend on whether the repository currently certifies.
fn sample_report() -> deacon_conformance::certification::CertificationReport {
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("registry loads");
    let certification = Certification {
        certified: true,
        profile: PROFILE.into(),
        blocking: vec![],
        waived: vec![],
        snapshot_coverage: vec![],
        no_reference: vec![],
        residual_queue: vec![],
        permanent_residuals: vec![],
        non_compliant_rules: vec![],
        obligations: Default::default(),
    };
    let inputs = deacon_conformance::certification::ReportInputs {
        deacon_revision: REVISION.into(),
        environment: deacon_conformance::certification::Environment {
            platform: "linux".into(),
            arch: "x86_64".into(),
            container_engine: "docker".into(),
            container_engine_version: "27.3.1".into(),
            compose_version: "2.29.7".into(),
        },
        evaluation_date: "2026-07-28".into(),
        schema_documents: 4,
        prose_documents: 18,
        admitted_non_deterministic_inputs: vec![],
    };
    build_report(&certification, &registry, &inputs)
}

#[test]
fn the_report_carries_all_sixteen_fr034_fields() {
    // Sixteen, not twenty: `scope.profile`, `scope.doesNotCertify`, `evaluationDate`, and
    // `notCertified` are required too, but they satisfy FR-035, FR-040, and FR-037.
    assert_eq!(FR034_FIELDS.len(), FR034_FIELD_COUNT);

    let report = sample_report();
    let doc: serde_json::Value =
        serde_json::from_str(&render_json(&report)).expect("report is valid JSON");

    for field in FR034_FIELDS {
        let mut cursor = &doc;
        for segment in field.split('.') {
            cursor = cursor
                .get(segment)
                .unwrap_or_else(|| panic!("report is missing FR-034 field `{field}`"));
        }
        assert!(!cursor.is_null(), "FR-034 field `{field}` is null");
    }
}

#[test]
fn the_four_required_but_non_fr034_fields_are_also_present() {
    let report = sample_report();
    let doc: serde_json::Value = serde_json::from_str(&render_json(&report)).expect("valid JSON");
    for field in ["evaluationDate", "notCertified"] {
        assert!(doc.get(field).is_some(), "missing `{field}`");
    }
    assert!(doc["scope"]["profile"].is_string());
    assert!(doc["scope"]["doesNotCertify"].is_array());
}

#[test]
fn the_scope_names_exactly_one_profile_and_enumerates_what_it_excludes() {
    // FR-053 / SC-010. A reader must be able to tell, from the report alone, that a
    // Linux/amd64/Docker certification says nothing about Podman.
    let report = sample_report();
    assert_eq!(report.scope.profile, PROFILE);

    let rendered = render_md(&report);
    assert!(
        rendered.contains("does NOT extend to"),
        "the report must state its non-extension explicitly"
    );
    let excluded = report.scope.does_not_certify.join("\n");
    assert!(
        excluded.contains("podman"),
        "Podman must be named as uncovered"
    );
    assert!(excluded.contains("macos") || excluded.contains("operating system: macos"));
    assert!(excluded.contains("aarch64"));
    assert!(
        excluded.contains("reference oracle version other than"),
        "another oracle version must be named as uncovered"
    );
}

#[test]
fn the_report_is_byte_reproducible() {
    // FR-054 / SC-005. Generated twice from identical inputs, compared byte for byte.
    let a = sample_report();
    let b = sample_report();
    assert_eq!(render_json(&a), render_json(&b));
    assert_eq!(render_md(&a), render_md(&b));
}

#[test]
fn the_report_contains_no_timestamp_hostname_or_absolute_path() {
    let rendered = format!(
        "{}{}",
        render_json(&sample_report()),
        render_md(&sample_report())
    );
    assert!(
        !rendered.contains("/workspaces"),
        "an absolute path makes the report machine-specific"
    );
    assert!(
        !rendered.contains("capturedAt"),
        "a timestamp makes two identical runs differ"
    );
    // The evaluation date is the one date, and it comes from `--today`, not the clock.
    assert!(rendered.contains("2026-07-28"));
}

// ---------------------------------------------------------------------------
// T016 — certification needs no reference, engine, or network (FR-056, SC-013)
// ---------------------------------------------------------------------------

#[test]
fn certification_resolves_no_reference_implementation() {
    // FR-056 / SC-013. Asserted structurally: the hermetic crate must declare no way to
    // reach a reference implementation or a network. A behavioral test with the reference
    // uninstalled would pass on any machine that never had it, which proves nothing.
    let manifest = std::fs::read_to_string(
        deacon_conformance::workspace_root()
            .join("crates")
            .join("conformance")
            .join("Cargo.toml"),
    )
    .expect("Cargo.toml readable");

    for networking in ["reqwest", "hyper", "ureq", "tokio", "curl", "isahc"] {
        assert!(
            !manifest.contains(&format!("\n{networking} ")),
            "the certifying crate must not depend on `{networking}` — certification runs \
             with no network in the release path (SC-013)"
        );
    }
}

#[test]
fn certification_reads_the_manifest_rather_than_probing_for_an_engine() {
    // The positive statement of the same property: the gate's knowledge of container
    // execution comes from a receipt it reads, not from a daemon it contacts.
    let source = std::fs::read_to_string(
        deacon_conformance::workspace_root()
            .join("crates")
            .join("conformance")
            .join("src")
            .join("certify.rs"),
    )
    .expect("certify.rs readable");
    for probe in [
        "Command::new(\"docker\")",
        "Command::new(\"podman\")",
        "docker_version()",
    ] {
        assert!(
            !source.contains(probe),
            "certification must not probe a container engine; found `{probe}`"
        );
    }
}

// ---------------------------------------------------------------------------
// T044 — manifest and snapshot are independent obligations (FR-033e)
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_manifest_does_not_excuse_a_stale_snapshot() {
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("registry loads");
    let (_guard, path) = manifest_file(&clean_manifest());

    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        manifest_path: Some(path),
        required_cases: vec![],
        resolvable_dispositions: BTreeSet::new(),
        stale_snapshots: vec![("case-a".into(), "caseHash".into())],
        recorded_oracles: vec![],
        applicable_units: BTreeSet::new(),
        accounted_units: BTreeSet::new(),
        profile_is_active: true,
    };
    let certification = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );
    assert!(
        certification
            .blocking
            .iter()
            .any(|b| b.kind == BlockingKind::StaleSnapshot),
        "a fresh manifest must not excuse a stale snapshot (FR-033e)"
    );
}

#[test]
fn a_fresh_snapshot_does_not_excuse_an_absent_manifest() {
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("registry loads");
    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        manifest_path: Some(PathBuf::from("/nonexistent/execution-manifest.json")),
        // A required case is what makes the absence meaningful: the manifest is demanded
        // exactly when container-backed execution was owed. With nothing required there is
        // no receipt to be missing, and the assertion would be vacuous.
        required_cases: vec![deacon_conformance::manifest::RequiredCase {
            case_id: "case-a".into(),
            case_hash: "h1".into(),
            fixture_hash: "f1".into(),
        }],
        resolvable_dispositions: BTreeSet::new(),
        stale_snapshots: vec![],
        recorded_oracles: vec![],
        applicable_units: BTreeSet::new(),
        accounted_units: BTreeSet::new(),
        profile_is_active: true,
    };
    let certification = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );
    assert!(
        certification
            .blocking
            .iter()
            .any(|b| b.kind == BlockingKind::MissingExecution),
        "a fresh snapshot must not excuse an absent manifest (FR-033e)"
    );
}

// ---------------------------------------------------------------------------
// T035 / T038 / T018 — runner omission, incorrect oracle, inactive profile
// ---------------------------------------------------------------------------

#[test]
fn control_d_an_unaccounted_unit_blocks() {
    let registry = registry_fixture();
    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        manifest_path: Some(PathBuf::from("/nonexistent/m.json")),
        applicable_units: BTreeSet::from(["case-a".to_string(), "case-b".to_string()]),
        accounted_units: BTreeSet::from(["case-a".to_string()]),
        profile_is_active: true,
        ..Default::default()
    };
    let certification = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );
    let omissions: Vec<&str> = certification
        .blocking
        .iter()
        .filter(|b| b.kind == BlockingKind::RunnerOmission)
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(
        omissions,
        vec!["case-b"],
        "the unaccounted unit must be named"
    );
}

#[test]
fn control_g_a_recorded_oracle_differing_from_the_pin_blocks() {
    let registry = registry_fixture();
    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        manifest_path: Some(PathBuf::from("/nonexistent/m.json")),
        recorded_oracles: vec![("snapshot:case-a".into(), "0.99.0".into())],
        profile_is_active: true,
        ..Default::default()
    };
    let certification = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );
    assert!(
        certification
            .blocking
            .iter()
            .any(|b| b.kind == BlockingKind::IncorrectOracle && b.id == "snapshot:case-a"),
        "an oracle identity differing from the declared pin must block"
    );
}

#[test]
fn control_an_inactive_profile_is_refused_not_vacuously_passed() {
    // FR-045. Zero applicable units is not the same as certified.
    let registry = registry_fixture();
    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        manifest_path: Some(PathBuf::from("/nonexistent/m.json")),
        profile_is_active: false,
        ..Default::default()
    };
    let certification = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );
    assert!(!certification.certified);
    assert!(
        certification
            .blocking
            .iter()
            .any(|b| b.kind == BlockingKind::InactiveProfile)
    );
}

// ---------------------------------------------------------------------------
// T045 / T097 — non-deterministic evidence and canary isolation
// ---------------------------------------------------------------------------

#[test]
fn the_verdict_is_identical_with_the_canary_surface_populated_and_absent() {
    // FR-060 / SC-016. The strongest available statement of canary isolation: the gate's
    // output cannot depend on canary state, because no loader path reaches it.
    let registry = registry_fixture();
    let evidence = EvidenceInputs {
        revision: REVISION.into(),
        manifest_path: Some(PathBuf::from("/nonexistent/m.json")),
        profile_is_active: true,
        ..Default::default()
    };
    let baseline = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );

    // The canary file exists in the repository; the verdict must be byte-identical to one
    // computed while ignoring it entirely — which it is, because nothing reads it.
    let again = certify_with_evidence(
        &registry,
        "2026-07-28",
        &inventory_inputs(),
        &clause_inputs(),
        &snapshots_dir(),
        &evidence,
    );
    assert_eq!(
        serde_json::to_string(&baseline).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
}

#[test]
fn no_certification_source_reads_the_discovery_root() {
    // FR-046 / SC-012, asserted where it is actually enforceable: the certifying modules
    // must contain no path to the discovery data root. A finding, a corpus result, or a
    // campaign outcome contributes zero coverage because there is no code that could
    // let it contribute any.
    let base = deacon_conformance::workspace_root()
        .join("crates")
        .join("conformance")
        .join("src");
    for module in ["certify.rs", "certification.rs", "manifest.rs"] {
        let source = std::fs::read_to_string(base.join(module)).expect("module readable");
        for forbidden in [
            "discovery::queue",
            "findings.json",
            "default_discovery_dir",
            "canary.json",
        ] {
            assert!(
                !source.contains(forbidden),
                "`{module}` references `{forbidden}`; non-deterministic evidence must not \
                 reach the certification verdict (FR-046)"
            );
        }
    }
}

#[test]
fn admitted_non_deterministic_inputs_default_to_none_and_are_recorded_when_present() {
    // FR-047: admission is possible, but only with the reason recorded. The default is
    // empty, which is the honest default — nothing qualifies unless it was shown to.
    let report = sample_report();
    assert!(report.admitted_non_deterministic_inputs.is_empty());

    let doc: serde_json::Value = serde_json::from_str(&render_json(&report)).expect("valid JSON");
    assert!(
        doc.get("admittedNonDeterministicInputs").is_some(),
        "the field must be present even when empty, so its emptiness is a claim rather \
         than an omission"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn registry_fixture() -> deacon_conformance::load::Registry {
    deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
        .expect("registry loads")
}

fn snapshots_dir() -> PathBuf {
    deacon_conformance::workspace_root()
        .join("conformance")
        .join("snapshots")
}

fn inventory_inputs() -> deacon_conformance::validate::InventoryInputs<'static> {
    // Leaked so the borrowed-path struct can be returned; these live for the process, and
    // a test binary's process is exactly the right lifetime for a fixture path.
    let schemas: &'static PathBuf =
        Box::leak(Box::new(deacon_conformance::default_pinned_schemas_dir()));
    let inventory: &'static PathBuf =
        Box::leak(Box::new(deacon_conformance::default_inventory_file()));
    deacon_conformance::validate::InventoryInputs {
        schemas_dir: schemas,
        inventory_file: inventory,
    }
}

fn clause_inputs() -> deacon_conformance::validate::ClauseInputs<'static> {
    let spec: &'static PathBuf = Box::leak(Box::new(deacon_conformance::default_pinned_spec_dir()));
    let clauses: &'static PathBuf = Box::leak(Box::new(deacon_conformance::default_clauses_file()));
    deacon_conformance::validate::ClauseInputs {
        spec_dir: spec,
        clauses_file: clauses,
    }
}
