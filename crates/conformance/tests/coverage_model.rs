//! Acceptance tests for User Story 1 — "See the shape of the hole"
//! (024-deterministic-conformance-coverage, T023–T030; FR-071).
//!
//! Hermetic: no network, no Docker, no reference oracle. Every test runs either against
//! the REAL committed registry — so it cannot pass against a convenient synthetic model
//! that no longer resembles what ships — or against a tempdir copy of it with exactly one
//! thing changed.
//!
//! Two of these tests deliberately re-implement part of the model rather than calling the
//! production evaluator ([`brute_force_excluded`], [`brute_force_valid_pairs`]). Calling
//! `scenario::is_invalid` to check `scenario::is_invalid`'s output would make the
//! assertion a tautology; a second, independent reading of the same records is what makes
//! it evidence.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use deacon_conformance::coverage::{ObligationBucket, evaluate_obligations};
use deacon_conformance::coverage_report::{CoverageReports, build_coverage_reports};
use deacon_conformance::load::Registry;
use deacon_conformance::obligation::{
    ObligationKind, generate_obligations, render as render_obligations,
};
use deacon_conformance::scenario::{ApplicabilityRule, OPERATION_DIMENSION, ScenarioDimension};
use deacon_conformance::validate::check_scenario_model;
use deacon_conformance::{default_registry_dir, workspace_root};

use support::Fixture;

fn real_registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("the real registry loads")
}

fn reports_for(registry: &Registry) -> CoverageReports {
    let inventory = generate_obligations(registry).expect("obligations generate");
    build_coverage_reports(registry, &inventory)
}

// ---------------------------------------------------------------------------
// Independent re-implementations (see the module doc)
// ---------------------------------------------------------------------------

/// Whether some rule forbids `combination`, read straight off the records without going
/// through `scenario.rs`. A rule bites only when EVERY condition it lists is pinned by
/// the combination to a value the condition names.
fn brute_force_excluded(
    rules: &[ApplicabilityRule],
    combination: &BTreeMap<&str, &str>,
) -> Option<String> {
    for rule in rules {
        if rule.excludes.is_empty() {
            continue;
        }
        let bites = rule.excludes.iter().all(|condition| {
            combination
                .get(condition.dimension.as_str())
                .is_some_and(|assigned| condition.values.iter().any(|v| v == assigned))
        });
        if bites {
            return Some(rule.id.clone());
        }
    }
    None
}

/// Every valid `(operation, d₁=v₁, d₂=v₂)` pair, enumerated independently.
fn brute_force_valid_pairs(
    dimensions: &[ScenarioDimension],
    rules: &[ApplicabilityRule],
) -> BTreeSet<(String, String, String, String, String)> {
    let operations = dimensions
        .iter()
        .find(|d| d.id == OPERATION_DIMENSION)
        .expect("the model declares an operation dimension");
    let pairable: Vec<&ScenarioDimension> = dimensions
        .iter()
        .filter(|d| d.id != OPERATION_DIMENSION)
        .collect();

    let mut out = BTreeSet::new();
    for operation in &operations.values {
        // A dimension every one of whose values is excluded with this operation is
        // inapplicable and contributes nothing.
        let permitted: Vec<(&ScenarioDimension, Vec<&String>)> = pairable
            .iter()
            .map(|dimension| {
                let values: Vec<&String> = dimension
                    .values
                    .iter()
                    .filter(|value| {
                        let combination = BTreeMap::from([
                            (OPERATION_DIMENSION, operation.as_str()),
                            (dimension.id.as_str(), value.as_str()),
                        ]);
                        brute_force_excluded(rules, &combination).is_none()
                    })
                    .collect();
                (*dimension, values)
            })
            .filter(|(_, values)| !values.is_empty())
            .collect();

        for (i, (first, first_values)) in permitted.iter().enumerate() {
            for (second, second_values) in permitted.iter().skip(i + 1) {
                for a in first_values {
                    for b in second_values {
                        let combination = BTreeMap::from([
                            (OPERATION_DIMENSION, operation.as_str()),
                            (first.id.as_str(), a.as_str()),
                            (second.id.as_str(), b.as_str()),
                        ]);
                        if brute_force_excluded(rules, &combination).is_some() {
                            continue;
                        }
                        out.insert((
                            operation.clone(),
                            first.id.clone(),
                            (*a).clone(),
                            second.id.clone(),
                            (*b).clone(),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// The pairs the pairwise report enumerated, in the same 5-tuple shape.
fn reported_pairs(reports: &CoverageReports) -> BTreeSet<(String, String, String, String, String)> {
    let mut out = BTreeSet::new();
    for operation in &reports.pairwise.operations {
        for pair in &operation.pairs {
            if pair.arity != 2 {
                continue;
            }
            let entries: Vec<(&String, &String)> = pair.assignment.iter().collect();
            assert_eq!(
                entries.len(),
                2,
                "an arity-2 obligation pins two dimensions"
            );
            out.insert((
                operation.operation.clone(),
                entries[0].0.clone(),
                entries[0].1.clone(),
                entries[1].0.clone(),
                entries[1].1.clone(),
            ));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// T023 — scenario 1
// ---------------------------------------------------------------------------

/// The report lists **every** valid combination and omits **every** rule-forbidden one.
///
/// Checked in both directions against an independent enumeration: a report that dropped a
/// valid pair would understate the hole, and one that kept a forbidden pair would make the
/// denominator unreachable — both failures look like progress from inside the report.
#[test]
fn the_report_lists_every_valid_combination_and_omits_every_forbidden_one() {
    let registry = real_registry();
    let reports = reports_for(&registry);

    let expected = brute_force_valid_pairs(&registry.scenario, &registry.applicability);
    let actual = reported_pairs(&reports);

    let missing: Vec<_> = expected.difference(&actual).take(5).collect();
    assert!(
        missing.is_empty(),
        "the report omits valid combination(s) an independent enumeration found: {missing:#?}"
    );
    let extra: Vec<_> = actual.difference(&expected).take(5).collect();
    assert!(
        extra.is_empty(),
        "the report lists combination(s) an applicability rule forbids: {extra:#?}"
    );
    assert!(
        !expected.is_empty(),
        "the enumeration must be non-empty, or the two-way comparison proves nothing"
    );

    // Every pair also states whether an executable case covers it — a bucket is never
    // absent and never a sixth, invented word.
    let vocabulary: BTreeSet<&str> = [
        ObligationBucket::Covered,
        ObligationBucket::Waived,
        ObligationBucket::NonTestable,
        ObligationBucket::Gap,
        ObligationBucket::InactiveEnvironment,
        ObligationBucket::Undispositioned,
    ]
    .iter()
    .map(|b| b.as_str())
    .collect();
    for operation in &reports.pairwise.operations {
        for pair in &operation.pairs {
            assert!(
                vocabulary.contains(pair.bucket.as_str()),
                "{} carries unknown bucket {:?}",
                pair.obligation,
                pair.bucket
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T024 — scenario 2
// ---------------------------------------------------------------------------

/// An excluded combination appears in **neither** population and names the rule that
/// excluded it.
///
/// This is the one place where "silently absent" and "explicitly excluded" must not be
/// confused: collapsing the two would make the denominator unfalsifiable, because any
/// missing combination could be explained away as impossible.
#[test]
fn an_excluded_combination_is_absent_from_both_populations_and_names_its_rule() {
    let registry = real_registry();
    let reports = reports_for(&registry);

    let read = reports
        .pairwise
        .operations
        .iter()
        .find(|o| o.operation == "read-configuration")
        .expect("read-configuration is a declared operation");

    // `read-configuration` never creates or inspects a container.
    let excluded = read
        .excluded
        .iter()
        .find(|e| e.assignment.get("sdim-container-state").map(String::as_str) == Some("running"))
        .expect("a running container is excluded for read-configuration");
    assert!(
        !excluded.rule.is_empty(),
        "the excluding rule id must travel with the exclusion (FR-012)"
    );
    assert!(
        registry.applicability.iter().any(|r| r.id == excluded.rule),
        "the named rule {:?} must resolve to a declared rule",
        excluded.rule
    );

    // It is in NEITHER the covered nor the uncovered population — i.e. in no pair at all.
    for pair in &read.pairs {
        assert_ne!(
            pair.assignment
                .get("sdim-container-state")
                .map(String::as_str),
            Some("running"),
            "an excluded combination must not appear as an obligation: {}",
            pair.obligation
        );
    }

    // And the whole inventory agrees — the exclusion is not merely hidden by the report.
    let inventory = generate_obligations(&registry).expect("obligations generate");
    for unit in &inventory.units {
        if unit.operation.as_deref() != Some("read-configuration") {
            continue;
        }
        if let Some(assignment) = unit.assignment.as_ref() {
            assert_ne!(
                assignment.get("sdim-container-state").map(String::as_str),
                Some("running"),
                "{} enumerates a forbidden combination",
                unit.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T025 — scenario 3, SC-010
// ---------------------------------------------------------------------------

/// Two generations from an unchanged record are byte-identical — the obligations and
/// every report artifact.
///
/// Byte equality, not structural equality: the committed inventory is gated by a byte
/// comparison (V27), so anything short of byte stability would make that gate fire on
/// nothing but formatting churn.
#[test]
fn two_generations_from_an_unchanged_record_are_byte_identical() {
    let first = real_registry();
    let second = real_registry();

    let a = render_obligations(&generate_obligations(&first).expect("generate"));
    let b = render_obligations(&generate_obligations(&second).expect("generate"));
    assert_eq!(a, b, "obligation generation must be byte-stable (SC-010)");

    let reports_a = reports_for(&first);
    let reports_b = reports_for(&second);
    assert_eq!(
        serde_json::to_string(&reports_a.pairwise).unwrap(),
        serde_json::to_string(&reports_b.pairwise).unwrap(),
        "coverage-pairwise must be byte-stable"
    );
    assert_eq!(
        serde_json::to_string(&reports_a.operations).unwrap(),
        serde_json::to_string(&reports_b.operations).unwrap(),
        "coverage-operations must be byte-stable"
    );
    assert_eq!(
        serde_json::to_string(&reports_a.observables).unwrap(),
        serde_json::to_string(&reports_b.observables).unwrap(),
        "coverage-observables must be byte-stable"
    );

    // The Markdown is rendered from the same ordered model, so it is stable too — and,
    // being derived, cannot disagree with the JSON about what is covered.
    assert_eq!(
        deacon_conformance::coverage_report::render_pairwise_md(&reports_a.pairwise),
        deacon_conformance::coverage_report::render_pairwise_md(&reports_b.pairwise),
    );
}

// ---------------------------------------------------------------------------
// T026 — scenario 4, FR-010
// ---------------------------------------------------------------------------

/// A value permitted by no rule in any combination is reported as **dead**, not silently
/// carried.
///
/// Dead values arise from rule edits — adding a rule can strand a value — which is exactly
/// when a silently-carried value would misrepresent the model's size.
#[test]
fn a_value_permitted_by_no_combination_is_reported_dead() {
    let operations: Vec<String> = real_registry()
        .scenario
        .iter()
        .find(|d| d.id == OPERATION_DIMENSION)
        .expect("operation dimension")
        .values
        .clone();

    // One added rule strands `sdim-features: lockfile` under every operation. Nothing
    // else changes: no dimension is edited, no case is touched.
    let fixture = Fixture::real().edit_registry_file("applicability.json", |doc| {
        let records = doc
            .get_mut("records")
            .and_then(|v| v.as_array_mut())
            .expect("applicability.json has records");
        records.push(serde_json::json!({
            "id": "rule-test-strands-the-lockfile-value",
            "excludes": [
                { "dimension": OPERATION_DIMENSION, "values": operations },
                { "dimension": "sdim-features", "values": ["lockfile"] }
            ],
            "ground": "Test-only rule that excludes the lockfile Feature value under every declared operation, so that the dead-value report has something to find."
        }));
    });

    let registry = fixture.registry();
    let reports = reports_for(&registry);

    assert!(
        reports
            .pairwise
            .dead_values
            .iter()
            .any(|d| d.dimension == "sdim-features" && d.value == "lockfile"),
        "a value excluded everywhere must be reported dead, got {:?}",
        reports.pairwise.dead_values
    );

    // And validation says so too: a dead value is V26, not a quiet omission.
    let violations = check_scenario_model(&registry);
    assert!(
        violations.iter().any(|v| v.code == "V26"
            && v.message.contains("DEAD")
            && v.message.contains("lockfile")),
        "a dead value must be a V26 violation, got {violations:#?}"
    );

    // The unmodified registry has none — the assertion above is not vacuously true.
    assert!(
        reports_for(&real_registry())
            .pairwise
            .dead_values
            .is_empty(),
        "the committed model must carry no dead values"
    );
}

// ---------------------------------------------------------------------------
// T027 — scenario 5
// ---------------------------------------------------------------------------

/// The behavior obligation this fixture pins to an inactive environment.
const PODMAN_BEHAVIOR: &str = "bhv-container-identity-labels";

/// Constrain one behavior to Podman while the Docker profile is the active one.
fn podman_only_behavior(fixture: Fixture) -> Fixture {
    fixture.edit_behavior(PODMAN_BEHAVIOR, |record| {
        record["applicability"] = serde_json::json!([
            { "dimension": "dim-runtime", "values": ["podman"] }
        ]);
    })
}

/// Obligations of a **modelled but inactive** environment are enumerated and reported
/// `inactive-environment` — counted as neither covered nor gap.
///
/// The unexercised environment has to be visible as a backlog. Dropping it from the
/// denominator would make the coverage percentage rise by removing work, which is the
/// arithmetic this feature exists to forbid.
#[test]
fn obligations_of_an_inactive_environment_are_enumerated_not_dropped() {
    let fixture = podman_only_behavior(Fixture::real());
    let registry = fixture.registry();
    let inventory = generate_obligations(&registry).expect("generate");

    let outcomes = evaluate_obligations(&registry, &inventory);
    let entry = outcomes
        .iter()
        .find(|o| o.obligation.behavior.as_deref() == Some(PODMAN_BEHAVIOR))
        .expect("the podman-only behavior still has an enumerated obligation");

    assert_eq!(
        entry.bucket,
        ObligationBucket::InactiveEnvironment,
        "a behavior outside the active profile buckets as inactive-environment"
    );
    assert!(
        entry.by.is_empty(),
        "an inactive-environment obligation claims no covering evidence"
    );

    let summary = &reports_for(&registry).pairwise.summary;
    assert_eq!(
        summary.inactive_environment, 1,
        "the inactive environment must be counted in its own bucket, never folded (FR-026)"
    );
    assert_eq!(
        summary.valid,
        inventory.units.len(),
        "an inactive-environment obligation stays IN the denominator; it is reported, not removed"
    );

    // Neither covered nor gap: the five buckets plus undispositioned partition the set.
    let total = summary.covered
        + summary.waived
        + summary.non_testable
        + summary.gap
        + summary.inactive_environment
        + summary.undispositioned;
    assert_eq!(
        total, summary.valid,
        "the buckets must partition the obligations"
    );
}

// ---------------------------------------------------------------------------
// T028 — scenario 6, SC-015, FR-004b
// ---------------------------------------------------------------------------

/// Marking the modelled environment **active** re-buckets its obligations with **zero**
/// changes to the model, the applicability rules, or any case.
///
/// Activation is a data change (FR-004b). If it required editing the scenario model or
/// re-authoring a case, the `inactive-environment` bucket would be a promise rather than a
/// backlog.
#[test]
fn activating_a_profile_rebuckets_its_obligations_with_no_other_change() {
    let inactive = podman_only_behavior(Fixture::real());
    let before = fingerprint_model_and_cases(&inactive);
    let before_bucket = bucket_of_podman_behavior(&inactive);
    assert_eq!(before_bucket, ObligationBucket::InactiveEnvironment);

    // The ONLY edit: swap which profile is active.
    let active = podman_only_behavior(Fixture::real()).edit_registry_file("profiles.json", |doc| {
        let records = doc
            .get_mut("records")
            .and_then(|v| v.as_array_mut())
            .expect("profiles.json has records");
        for record in records.iter_mut() {
            record["active"] = serde_json::Value::Bool(false);
        }
        let mut podman = records[0].clone();
        podman["id"] = serde_json::json!("prof-linux-amd64-podman-0870");
        podman["context"]["dim-runtime"] = serde_json::json!("podman");
        podman["active"] = serde_json::Value::Bool(true);
        records.push(podman);
    });

    let after = fingerprint_model_and_cases(&active);
    assert_eq!(
        before, after,
        "activating a profile must not change scenario.json, applicability.json, or any case"
    );

    let after_bucket = bucket_of_podman_behavior(&active);
    assert_ne!(
        after_bucket,
        ObligationBucket::InactiveEnvironment,
        "the obligation must re-bucket once its environment is active"
    );
    assert_eq!(
        after_bucket,
        ObligationBucket::Covered,
        "with evidence present it re-buckets to covered; without it, to undispositioned"
    );

    let summary = &reports_for(&active.registry()).pairwise.summary;
    assert_eq!(
        summary.inactive_environment, 0,
        "no obligation stays inactive once its environment is the active one"
    );
}

/// The bytes of everything FR-004b says activation must NOT require changing.
fn fingerprint_model_and_cases(fixture: &Fixture) -> Vec<(String, String)> {
    let registry_dir = fixture.registry_dir();
    let mut out = Vec::new();
    for name in ["scenario.json", "applicability.json"] {
        out.push((
            name.to_string(),
            std::fs::read_to_string(registry_dir.join(name)).expect("read"),
        ));
    }
    let cases = registry_dir.join("cases");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&cases)
        .expect("read cases dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for file in files {
        out.push((
            format!("cases/{}", file.file_name().unwrap().to_string_lossy()),
            std::fs::read_to_string(&file).expect("read"),
        ));
    }
    out
}

fn bucket_of_podman_behavior(fixture: &Fixture) -> ObligationBucket {
    let registry = fixture.registry();
    let inventory = generate_obligations(&registry).expect("generate");
    let outcomes = evaluate_obligations(&registry, &inventory);
    outcomes
        .iter()
        .find(|o| o.obligation.behavior.as_deref() == Some(PODMAN_BEHAVIOR))
        .expect("the behavior obligation exists in both runs")
        .bucket
}

// ---------------------------------------------------------------------------
// T029 — FR-013
// ---------------------------------------------------------------------------

/// The full Cartesian product is never materialized: the combination-obligation count
/// equals the enumerated valid-pair count, not the product of all dimension sizes.
#[test]
fn the_full_cartesian_product_is_never_materialized() {
    let registry = real_registry();
    let inventory = generate_obligations(&registry).expect("generate");

    let combinations = inventory
        .units
        .iter()
        .filter(|u| u.kind == ObligationKind::Combination)
        .count();
    let expected = brute_force_valid_pairs(&registry.scenario, &registry.applicability).len();
    assert_eq!(
        combinations, expected,
        "the obligation count must equal the enumerated valid-pair count"
    );

    let product: usize = registry.scenario.iter().map(|d| d.values.len()).product();
    assert!(
        combinations < product,
        "the pair enumeration ({combinations}) must be smaller than the Cartesian product \
         ({product}); a count at or above it would mean the product was materialized"
    );

    // Every combination obligation pins exactly two dimensions (a triple pins three, and
    // none is selected yet) — the shape that makes the space tractable.
    for unit in inventory
        .units
        .iter()
        .filter(|u| u.kind == ObligationKind::Combination)
    {
        let arity = unit
            .arity
            .expect("a combination obligation declares an arity");
        assert_eq!(
            unit.assignment.as_ref().map(|a| a.len()),
            Some(arity as usize),
            "{} pins a number of dimensions that disagrees with its arity",
            unit.id
        );
        assert!(
            arity == 2 || arity == 3,
            "{} declares arity {arity}; only pairs and hand-selected triples exist",
            unit.id
        );
    }
}

// ---------------------------------------------------------------------------
// T030 — FR-018
// ---------------------------------------------------------------------------

/// `coverage generate` writes **only** `obligations/obligations.json`.
///
/// Asserted by fingerprinting the whole registry tree before and after a real generation
/// run, so the guarantee covers files this test never thought to name. A generator that
/// could edit a disposition, a case, a behavior, a waiver, or a gap would convert human
/// review into a build artifact — the 020/021 boundary, restated because it is the
/// invariant most easily lost.
#[test]
fn generation_writes_only_the_machine_owned_obligation_inventory() {
    let fixture = Fixture::real();
    let registry_dir = fixture.registry_dir();
    let obligations = fixture.obligations_file();

    // Start from a stale inventory so the run has something to change.
    std::fs::write(&obligations, "{}\n").expect("seed a stale inventory");

    let before = fingerprint_tree(&fixture.conformance_dir());

    let status = std::process::Command::new(env!("CARGO_BIN_EXE_conformance"))
        .args(["--registry", &registry_dir.display().to_string()])
        .args(["coverage", "generate"])
        .current_dir(workspace_root())
        .status()
        .expect("run `coverage generate`");
    assert!(status.success(), "generation must succeed: {status:?}");

    let after = fingerprint_tree(&fixture.conformance_dir());

    let changed: Vec<&String> = before
        .iter()
        .filter(|(path, digest)| after.get(*path) != Some(digest))
        .map(|(path, _)| path)
        .collect();
    let added: Vec<&String> = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .collect();

    assert_eq!(
        changed,
        vec![&"obligations/obligations.json".to_string()],
        "generation must touch exactly one file; also added: {added:?}"
    );
    assert!(
        added.is_empty(),
        "generation must create no other file, got {added:?}"
    );

    // Spelt out, so a future refactor that starts writing one of these fails by name.
    let inventory: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&obligations).expect("read")).expect("parse");
    assert!(
        inventory["units"].as_array().is_some_and(|u| !u.is_empty()),
        "the run must actually have produced obligations, or 'nothing else changed' is trivial"
    );
    for protected in [
        "registry/cases",
        "registry/behaviors",
        "registry/waivers",
        "registry/gaps.json",
        "registry/scenario.json",
        "registry/applicability.json",
        "registry/obligation-dispositions",
    ] {
        for (path, digest) in &before {
            if path.starts_with(protected) {
                assert_eq!(
                    after.get(path),
                    Some(digest),
                    "generation modified {path}, which is hand-authored"
                );
            }
        }
    }
}

/// `relative path → content digest` for every file under `root`, deterministically.
fn fingerprint_tree(root: &std::path::Path) -> BTreeMap<String, String> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut BTreeMap<String, String>) {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                walk(root, &entry, out);
            } else {
                let bytes = std::fs::read(&entry).expect("read file");
                let relative = entry
                    .strip_prefix(root)
                    .expect("path is under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, format!("{:x}", md5_like(&bytes)));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// A cheap content digest — this is change detection inside a test, not a security
/// boundary, so a 128-bit FNV-1a keeps the test free of another dependency.
fn md5_like(bytes: &[u8]) -> u128 {
    let mut hash: u128 = 0x6c62272e07bb0142_62b821756295c58d;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000000001000000_000000000000013B);
    }
    hash
}
