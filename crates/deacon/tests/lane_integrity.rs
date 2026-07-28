//! Lane-integrity acceptance tests (026-continuous-conformance-certification, US3).
//!
//! Hermetic: no container engine, no reference oracle, no network. Runs in the `default`
//! and `dev-fast` profiles, so a lane defect blocks the pull request that introduced it.
//!
//! What these tests are actually defending: a lane taxonomy is only worth having if a
//! green lane means something specific. Every test here attacks one way that meaning could
//! quietly erode — a unit assigned to nothing, a program a profile does not really select,
//! a lane that could write to the record, or a report whose exit code starts depending on
//! what it found.

use std::collections::BTreeSet;

use deacon_conformance::lane::{
    DISCOVERY_VALIDATION_CLASSES, ExecutionUnit, Lane, REGISTRY_VALIDATION_CLASSES, UnitKind,
    build_lane_report, case_memberships, check_lanes, derive_conformance_programs,
    derive_execution_units, load_lanes,
};
use deacon_conformance::load::Registry;
use deacon_conformance::{default_registry_dir, lanes_dir_for, workspace_root};

fn lanes() -> Vec<Lane> {
    load_lanes(&lanes_dir_for(&default_registry_dir())).expect("lanes.json loads")
}

fn registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("registry loads")
}

fn tests_dir() -> std::path::PathBuf {
    workspace_root().join("crates").join("deacon").join("tests")
}

fn snapshots_dir() -> std::path::PathBuf {
    workspace_root().join("conformance").join("snapshots")
}

fn units(registry: &Registry) -> Vec<ExecutionUnit> {
    derive_execution_units(registry, &tests_dir(), &snapshots_dir())
}

// ---------------------------------------------------------------------------
// T057 — each lane's inclusion rule (FR-049)
// ---------------------------------------------------------------------------

#[test]
fn exactly_five_lanes_are_declared() {
    let lanes = lanes();
    assert_eq!(lanes.len(), 5, "FR-001 declares exactly five lanes");
    let ids: BTreeSet<&str> = lanes.iter().map(|l| l.id.as_str()).collect();
    for expected in [
        "lane-pr-hermetic",
        "lane-pr-docker",
        "lane-nightly-stable",
        "lane-canary",
        "lane-release-certification",
    ] {
        assert!(ids.contains(expected), "missing lane `{expected}`");
    }
}

#[test]
fn every_lane_states_what_it_selects_and_what_it_excludes() {
    // FR-049 wants both halves asserted. A lane that lists what it runs but not what it
    // leaves out produces a green status a reader cannot interpret.
    for lane in lanes() {
        assert!(
            !lane.excludes.rationale.trim().is_empty(),
            "lane `{}` must state why it excludes what it excludes (FR-005)",
            lane.id
        );
        let selects_something = !lane.includes.validation_classes.is_empty()
            || !lane.includes.programs.is_empty()
            || lane.includes.case_predicate.is_some()
            || lane.includes.snapshot_replay;
        assert!(
            selects_something,
            "lane `{}` selects nothing at all",
            lane.id
        );
    }
}

#[test]
fn the_hermetic_lane_declares_no_preconditions_and_the_live_lanes_do() {
    let lanes = lanes();
    let find = |id: &str| {
        lanes
            .iter()
            .find(|l| l.id == id)
            .expect("lane exists")
            .clone()
    };

    assert!(
        find("lane-pr-hermetic").preconditions.is_empty(),
        "the hermetic lane must need no container engine, oracle, or network (FR-008)"
    );
    assert!(
        find("lane-release-certification").preconditions.is_empty(),
        "certification must run with nothing installed (FR-033a, SC-013)"
    );
    assert!(
        !find("lane-pr-docker").preconditions.is_empty(),
        "the container lane must declare its engine precondition so FR-004 can fail loudly"
    );
    assert!(
        !find("lane-nightly-stable").preconditions.is_empty(),
        "the nightly lane needs the pinned reference"
    );
}

#[test]
fn the_container_lane_never_declares_the_reference_oracle() {
    // FR-012: the container pull-request lane runs deacon against pinned expected
    // observables. Declaring the oracle would make a pull request depend on upstream
    // availability, which is exactly what pinning the observables buys us.
    use deacon_conformance::lane::Precondition;
    let lanes = lanes();
    let docker = lanes
        .iter()
        .find(|l| l.id == "lane-pr-docker")
        .expect("lane exists");
    assert!(
        !docker
            .preconditions
            .contains(&Precondition::ReferenceOracle),
        "FR-012 forbids the container pull-request lane from resolving the reference"
    );
}

// ---------------------------------------------------------------------------
// T058 — the denominator is machine-derived (FR-059, SC-001)
// ---------------------------------------------------------------------------

#[test]
fn every_derived_unit_is_assigned_to_at_least_one_lane() {
    let registry = registry();
    let units = units(&registry);
    let memberships = case_memberships(&registry);
    let violations = check_lanes(&lanes(), &units, &memberships, None);
    let unassigned: Vec<&str> = violations
        .iter()
        .filter(|v| v.message.contains("assigned to zero lanes"))
        .map(|v| v.record.as_str())
        .collect();
    assert!(
        unassigned.is_empty(),
        "SC-001 requires zero unassigned units; found: {unassigned:?}"
    );
}

#[test]
fn a_new_program_enters_the_denominator_with_no_hand_edit() {
    // FR-059's actual claim: introducing a unit must fail validation *without* anyone
    // editing a list. A hand-authored denominator would let an omitted unit satisfy
    // "every unit is assigned" while being covered by nothing — the check inverted into a
    // rubber stamp.
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        dir.path().join("plain_integration.rs"),
        "#[test]\nfn t() {}\n",
    )
    .expect("write");
    std::fs::write(
        dir.path().join("uses_conformance.rs"),
        "use deacon_conformance::load::Registry;\n#[test]\nfn t() {}\n",
    )
    .expect("write");

    let programs = derive_conformance_programs(dir.path());
    assert!(
        programs.contains(&"uses_conformance".to_string()),
        "a program referencing the conformance crate must enter the denominator"
    );
    assert!(
        !programs.contains(&"plain_integration".to_string()),
        "an ordinary integration test must NOT — this feature does not claim authority \
         over the whole test suite"
    );
}

#[test]
fn an_unassigned_unit_is_reported_as_v34() {
    let registry = registry();
    let mut units = units(&registry);
    units.push(ExecutionUnit {
        id: "unit-prog-invented_binary".into(),
        kind: UnitKind::Program,
        subject: "invented_binary".into(),
    });
    let violations = check_lanes(&lanes(), &units, &case_memberships(&registry), None);
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V34" && v.record == "unit-prog-invented_binary"),
        "an unassigned unit must be a V34 violation"
    );
}

#[test]
fn both_class_enumerations_contribute_to_the_denominator() {
    // data-model §2: the registry V-series AND the discovery D-series. A D-class is as
    // much an independent outcome as a V-class, and omitting one would let a whole
    // validator run in no lane.
    let registry = registry();
    let units = units(&registry);
    let classes: BTreeSet<&str> = units
        .iter()
        .filter(|u| u.kind == UnitKind::ValidationClass)
        .map(|u| u.subject.as_str())
        .collect();
    for class in REGISTRY_VALIDATION_CLASSES
        .iter()
        .chain(DISCOVERY_VALIDATION_CLASSES)
    {
        assert!(
            classes.contains(class),
            "class `{class}` missing from denominator"
        );
    }
    assert!(
        classes.contains("D6"),
        "the canary-pin class must be in the denominator"
    );
}

// ---------------------------------------------------------------------------
// T059 — explicit allow-lists, and profile agreement (FR-002)
// ---------------------------------------------------------------------------

#[test]
fn program_selection_is_an_explicit_allow_list_with_no_pattern_syntax() {
    // FR-002. A glob in a program list would silently capture a new binary or silently
    // drop a renamed one — the mistake the parity and discovery nextest profiles have each
    // documented making.
    for lane in lanes() {
        for program in &lane.includes.programs {
            assert!(
                !program.contains('*') && !program.contains('#') && !program.contains('?'),
                "lane `{}` selects programs by pattern `{program}`; FR-002 requires an \
                 explicit allow-list",
                lane.id
            );
        }
        for class in &lane.includes.validation_classes {
            assert!(
                !class.contains('*'),
                "lane `{}` selects validation classes by pattern `{class}`",
                lane.id
            );
        }
    }
}

#[test]
fn every_declared_program_exists_in_the_test_tree() {
    let available: BTreeSet<String> = derive_conformance_programs(&tests_dir())
        .into_iter()
        .collect();
    for lane in lanes() {
        for program in &lane.includes.programs {
            assert!(
                available.contains(program),
                "lane `{}` declares program `{program}`, which does not exist",
                lane.id
            );
        }
    }
}

#[test]
fn each_declared_nextest_profile_exists() {
    let raw = std::fs::read_to_string(workspace_root().join(".config").join("nextest.toml"))
        .expect("nextest.toml readable");
    for lane in lanes() {
        for profile in &lane.nextest_profiles {
            assert!(
                raw.contains(&format!("[profile.{profile}]")),
                "lane `{}` declares profile `{profile}`, which `.config/nextest.toml` does \
                 not define",
                lane.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T060 — blocking and write-capability invariants (FR-015, FR-016, FR-019, FR-020)
// ---------------------------------------------------------------------------

#[test]
fn no_lane_may_write_to_the_record() {
    // FR-016 / FR-020. The field is carried explicitly rather than simply absent so a
    // future lane that wants to write has to change a value a reviewer sees.
    for lane in lanes() {
        assert!(
            !lane.may_write_record,
            "lane `{}` declares mayWriteRecord: true",
            lane.id
        );
    }
}

#[test]
fn the_nightly_and_canary_lanes_are_non_blocking() {
    let lanes = lanes();
    for id in ["lane-nightly-stable", "lane-canary"] {
        let lane = lanes.iter().find(|l| l.id == id).expect("lane exists");
        assert!(
            !lane.blocking,
            "lane `{id}` must be non-blocking (FR-015/FR-019): its status reflects \
             whether it ran, not what it found"
        );
    }
}

#[test]
fn the_pull_request_and_release_lanes_block() {
    let lanes = lanes();
    for id in [
        "lane-pr-hermetic",
        "lane-pr-docker",
        "lane-release-certification",
    ] {
        let lane = lanes.iter().find(|l| l.id == id).expect("lane exists");
        assert!(lane.blocking, "lane `{id}` must block (FR-009, FR-011)");
    }
}

// ---------------------------------------------------------------------------
// T061 — the case predicates partition the case space (FR-002a)
// ---------------------------------------------------------------------------

#[test]
fn every_case_belongs_to_exactly_one_lane() {
    let registry = registry();
    let memberships = case_memberships(&registry);
    assert!(
        !memberships.is_empty(),
        "the registry has declarative cases"
    );

    let lanes = lanes();
    for (case_id, membership) in &memberships {
        let matching: Vec<&str> = lanes
            .iter()
            .filter(|l| {
                l.includes.case_predicate.as_ref().is_some_and(|p| {
                    p.oracle_types.contains(&membership.oracle_type)
                        && p.resource_groups.contains(&membership.resource_group)
                })
            })
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "case `{case_id}` matches {matching:?}; the predicates must partition the case \
             space with no overlap and no remainder (FR-002a)"
        );
    }
}

#[test]
fn a_predicate_may_capture_a_new_case_but_never_drop_one() {
    // The asymmetry that makes derived selection safe: silent capture is intended (that is
    // what removes the per-case edit), silent drop is not. The partition proof is what
    // rules the second one out.
    let registry = registry();
    let violations = check_lanes(
        &lanes(),
        &units(&registry),
        &case_memberships(&registry),
        None,
    );
    assert!(
        !violations
            .iter()
            .any(|v| v.message.contains("matches no lane")),
        "a case selected by nothing runs nowhere"
    );
}

// ---------------------------------------------------------------------------
// T062 — a missing precondition fails loudly (FR-004)
// ---------------------------------------------------------------------------

#[test]
fn a_lane_declares_its_preconditions_so_a_missing_one_can_fail_rather_than_skip() {
    // FR-004's enforcement lives in the harness (a missing engine or a mismatched oracle
    // raises a cause-specific error). What is checkable hermetically is that every lane
    // needing a capability *declares* it — an undeclared precondition is what makes a
    // silent skip possible in the first place.
    use deacon_conformance::lane::Precondition;
    let lanes = lanes();
    let registry = registry();
    let memberships = case_memberships(&registry);

    for lane in &lanes {
        let Some(predicate) = lane.includes.case_predicate.as_ref() else {
            continue;
        };
        let selected: Vec<_> = memberships
            .values()
            .filter(|m| {
                predicate.oracle_types.contains(&m.oracle_type)
                    && predicate.resource_groups.contains(&m.resource_group)
            })
            .collect();
        if selected.iter().any(|m| m.needs_container) {
            assert!(
                lane.preconditions.contains(&Precondition::ContainerEngine),
                "lane `{}` selects a container-backed case but declares no engine \
                 precondition, so a missing engine could read as a skip",
                lane.id
            );
        }
        if selected.iter().any(|m| m.needs_oracle) {
            assert!(
                lane.preconditions.contains(&Precondition::ReferenceOracle),
                "lane `{}` selects a live-differential case but declares no oracle \
                 precondition",
                lane.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T063 — the report states ran vs excluded, and never gates (FR-005)
// ---------------------------------------------------------------------------

#[test]
fn the_lane_report_states_both_what_ran_and_what_was_excluded() {
    let registry = registry();
    let units = units(&registry);
    let report = build_lane_report(&lanes(), &units, &case_memberships(&registry));

    assert_eq!(report.lanes.len(), 5);
    for lane in &report.lanes {
        assert_eq!(
            lane.selected.len() + lane.excluded.len(),
            units.len(),
            "lane `{}` must account for every unit as either selected or excluded — \
             omission is what lets a green status imply coverage it did not provide",
            lane.id
        );
        assert!(
            !lane.exclusion_rationale.trim().is_empty(),
            "lane `{}` must say why it excludes what it excludes",
            lane.id
        );
    }
    assert!(
        report.unassigned.is_empty(),
        "unassigned units: {:?}",
        report.unassigned
    );
}

#[test]
fn the_lane_report_is_byte_stable() {
    let registry = registry();
    let units = units(&registry);
    let memberships = case_memberships(&registry);
    let a = build_lane_report(&lanes(), &units, &memberships);
    let b = build_lane_report(&lanes(), &units, &memberships);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}

// ---------------------------------------------------------------------------
// T064 — no unit reaches an ignored, pending, or skipped state (FR-006)
// ---------------------------------------------------------------------------

#[test]
fn no_conformance_owned_program_carries_an_ignore_attribute() {
    // FR-006. `#[ignore]` is precisely the "pending" state the requirement forbids: the
    // test exists, the lane reports green, and nothing ran. The harness's value is
    // truthful non-selection, which an ignored test silently undoes.
    let mut offenders = Vec::new();
    for program in derive_conformance_programs(&tests_dir()) {
        let path = tests_dir().join(format!("{program}.rs"));
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[ignore")
                || trimmed.starts_with("#[cfg_attr(") && trimmed.contains("ignore")
            {
                offenders.push(format!("{program}.rs:{}", lineno + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "conformance-owned programs must not mark units ignored or pending (FR-006): {offenders:?}"
    );
}

// ---------------------------------------------------------------------------
// T066 — no lane but the reviewed record path writes a committed snapshot (FR-055)
// ---------------------------------------------------------------------------

/// The one program permitted to write a committed snapshot: the reviewed record path.
const SNAPSHOT_WRITER: &str = "conformance-snapshot";

#[test]
fn no_lane_program_holds_a_write_path_to_the_committed_snapshot_tree() {
    // FR-055 / SC-006 — the "prevention of automatic snapshot blessing" guarantee.
    //
    // Asserted by source scan as well as behaviorally, because the property that matters
    // is that the *capability* is absent, not that it happens to be unused. A program that
    // could write a snapshot is one refactor from doing so on every continuous-integration
    // run, at which point committed evidence stops being reviewed evidence.
    let writer_markers = [
        "write_snapshot",
        "refresh_snapshot",
        "snapshot::write",
        "write_provenance",
    ];
    // The guards themselves are exempt, and have to be: a test that forbids a capability
    // must name it, so scanning for the name would make every such guard its own
    // violation. Their exemption is safe precisely because their content is the assertion
    // that nothing else has the capability.
    let guards = [
        "lane_integrity",
        "discovery_hermetic",
        "drift_hermetic",
        // `canary_lane` asserts the same property about itself (FR-020), so it names the
        // same markers. A guard that scanned its peers' assertions would report every one
        // of them as the violation they exist to prevent.
        "canary_lane",
    ];

    let mut offenders = Vec::new();
    for program in derive_conformance_programs(&tests_dir()) {
        if guards.contains(&program.as_str()) {
            continue;
        }
        let path = tests_dir().join(format!("{program}.rs"));
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for marker in writer_markers {
            if source.contains(marker) {
                offenders.push(format!("{program} references `{marker}`"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "only `{SNAPSHOT_WRITER} refresh` may write a committed snapshot (FR-055): {offenders:?}"
    );
}

#[test]
fn no_lane_declares_itself_able_to_write_the_record() {
    // The declarative half of the same guarantee: the lane model itself must not admit a
    // writing lane, so the property is visible in data as well as in code.
    assert!(lanes().iter().all(|l| !l.may_write_record));
}

// ---------------------------------------------------------------------------
// T096 / T100 — stable-tree isolation for the canary and nightly lanes
// ---------------------------------------------------------------------------

/// A content digest over the trees a lane must never modify: the registry, the committed
/// snapshots, and the pins.
fn stable_tree_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut paths = Vec::new();
    for root in [
        workspace_root().join("conformance").join("registry"),
        workspace_root().join("conformance").join("snapshots"),
    ] {
        collect_files(&root, &mut paths);
    }
    paths.push(
        workspace_root()
            .join("fixtures")
            .join("parity-corpus")
            .join("oracle.json"),
    );
    paths.sort();

    let mut hasher = Sha256::new();
    for path in paths {
        if let Ok(bytes) = std::fs::read(&path) {
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(&bytes);
        }
    }
    format!("{:x}", hasher.finalize())
}

fn collect_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn reading_the_canary_pin_surface_does_not_touch_the_stable_tree() {
    // FR-050 / SC-008. The full behavioral test needs the live canary lane; what is
    // checkable hermetically — and what actually guards the property day to day — is that
    // loading and reading canary state leaves the stable tree byte-identical.
    let before = stable_tree_digest();
    let canary = deacon_conformance::default_canary_file();
    let _ = std::fs::read_to_string(&canary);
    let registry = registry();
    let _ = build_lane_report(&lanes(), &units(&registry), &case_memberships(&registry));
    let after = stable_tree_digest();
    assert_eq!(
        before, after,
        "reading canary state must leave the registry, snapshots, and pins byte-identical"
    );
}

#[test]
fn the_nightly_lane_declares_no_write_capability() {
    // FR-016. The nightly lane surfaces divergences; it records none of them.
    let lanes = lanes();
    let nightly = lanes
        .iter()
        .find(|l| l.id == "lane-nightly-stable")
        .expect("lane exists");
    assert!(!nightly.may_write_record);
    assert!(
        !nightly.blocking,
        "a lane that cannot write and does not block cannot change the record either way"
    );
}

// ---------------------------------------------------------------------------
// T065 — the hermetic lane's own hermeticity (FR-008)
// ---------------------------------------------------------------------------

#[test]
fn the_hermetic_lane_declares_no_capability_it_must_not_have() {
    // FR-008. Asserted about the lane's DECLARATION rather than about the machine: an
    // environment probe would pass on any runner that happens to lack Docker, which proves
    // nothing about what the lane requires. What is checkable — and what actually holds the
    // property — is that the lane declares no precondition and selects no unit that needs one.
    use deacon_conformance::lane::Precondition;
    let lanes = lanes();
    let hermetic = lanes
        .iter()
        .find(|l| l.id == "lane-pr-hermetic")
        .expect("lane exists");

    assert!(
        hermetic.preconditions.is_empty(),
        "the hermetic lane must declare no container engine, reference oracle, or network"
    );

    let registry = registry();
    let memberships = case_memberships(&registry);
    let predicate = hermetic
        .includes
        .case_predicate
        .as_ref()
        .expect("the hermetic lane selects cases");
    for (case_id, membership) in &memberships {
        if !predicate.oracle_types.contains(&membership.oracle_type)
            || !predicate
                .resource_groups
                .contains(&membership.resource_group)
        {
            continue;
        }
        assert!(
            !membership.needs_oracle,
            "case `{case_id}` is selected by the hermetic lane but needs the reference \
             implementation — the lane would then fail on a runner that has none, which is \
             the silent-precondition failure FR-008 forbids"
        );
        assert!(
            !membership.needs_container,
            "case `{case_id}` is selected by the hermetic lane but needs a container engine"
        );
    }

    for other in &lanes {
        if other.id == "lane-pr-hermetic" || other.id == "lane-release-certification" {
            continue;
        }
        assert!(
            !other.preconditions.is_empty(),
            "lane `{}` runs live units but declares no precondition, so a missing \
             capability could read as a skip (FR-004)",
            other.id
        );
        assert!(
            !other.preconditions.contains(&Precondition::ReferenceOracle)
                || other.id != "lane-pr-docker",
            "the container pull-request lane must never declare the oracle (FR-012)"
        );
    }
}

#[test]
fn the_certifying_lane_needs_nothing_installed() {
    // The same property for the release lane (FR-033a, SC-013). Certification reads
    // committed data and a receipt; needing an engine or a reference would put the release
    // path back on capabilities the whole design removes.
    let lanes = lanes();
    let release = lanes
        .iter()
        .find(|l| l.id == "lane-release-certification")
        .expect("lane exists");
    assert!(release.preconditions.is_empty());
    assert!(
        release.nextest_profiles.is_empty(),
        "certification runs no test binary at all — it validates data and reads a manifest"
    );
}
