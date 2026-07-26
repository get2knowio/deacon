//! Acceptance tests for User Story 2 — "Nothing applicable stays unclassified"
//! (024-deterministic-conformance-coverage, T052–T060; FR-071, FR-078).
//!
//! Hermetic: no network, no Docker, no reference oracle. Every test runs against a
//! composed fixture registry under `fixtures/conformance/obligation-gating/` — `base/`
//! plus at most one variant directory, copied into a tempdir (see that tree's README for
//! why the fixtures are shaped that way).
//!
//! The tests drive the LIBRARY gate ([`certify`], [`check_obligation_dispositions`])
//! rather than the `conformance` binary, for one reason worth stating: the binary runs
//! full validation *before* certification and exits 1 on any violation, so every scenario
//! here would "fail certification" whether or not the obligation gate existed. Asserting
//! on the verdict's SHAPE — which blocker, carrying which class, naming which record — is
//! what distinguishes a working gate from a registry that merely fails.

use std::path::{Path, PathBuf};

use deacon_conformance::certify::{BlockingKind, Certification, certify};
use deacon_conformance::load::Registry;
use deacon_conformance::obligation::{generate_obligations, write_obligations};
use deacon_conformance::validate::{
    ClauseInputs, InventoryInputs, Violation, check_obligation_dispositions,
};
use deacon_conformance::workspace_root;
use tempfile::TempDir;

/// A fixed injected "today", before every fixture waiver's `expires`.
const TODAY: &str = "2026-07-19";
/// A "today" AFTER every fixture waiver's `expires` (2027-01-19).
const AFTER_EXPIRY: &str = "2028-01-01";

fn fixtures() -> PathBuf {
    workspace_root().join("fixtures/conformance/obligation-gating")
}

/// A composed registry: `base/` plus one variant, in a tempdir, with the machine-owned
/// obligation inventory generated into the sibling `obligations/` the loader expects.
struct Fixture {
    _tmp: TempDir,
    registry_dir: PathBuf,
}

impl Fixture {
    /// Compose `base` alone.
    fn base() -> Fixture {
        Fixture::variant(None)
    }

    /// Compose `base` overlaid with `variants/<name>`.
    fn with(name: &str) -> Fixture {
        Fixture::variant(Some(name))
    }

    fn variant(name: Option<&str>) -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry_dir = tmp.path().join("registry");
        copy_dir(&fixtures().join("base"), &registry_dir);
        if let Some(name) = name {
            let overlay = fixtures().join("variants").join(name);
            assert!(overlay.is_dir(), "no such variant: {}", overlay.display());
            copy_dir(&overlay, &registry_dir);
        }

        // The obligation inventory is machine-owned and a pure function of the registry
        // (V27), so it is generated here rather than committed once per variant.
        let registry = Registry::load(&registry_dir).expect("the composed fixture loads");
        let inventory = generate_obligations(&registry).expect("obligations generate");
        write_obligations(
            &tmp.path().join("obligations").join("obligations.json"),
            &inventory,
        )
        .expect("obligation inventory writes");

        Fixture {
            _tmp: tmp,
            registry_dir,
        }
    }

    fn registry(&self) -> Registry {
        Registry::load(&self.registry_dir).expect("the composed fixture loads")
    }

    /// The V-class violations the obligation gate reports.
    fn violations(&self) -> Vec<Violation> {
        check_obligation_dispositions(&self.registry())
    }

    /// The certification verdict at `today`. The constraint/clause joins are scoped out
    /// (these fixtures ship no vendored schemas or prose), so the verdict is exactly the
    /// gap / uncovered / obligation gate.
    fn certify(&self, today: &str) -> Certification {
        certify(
            &self.registry(),
            today,
            &InventoryInputs {
                schemas_dir: Path::new("/nonexistent-conformance/schemas"),
                inventory_file: Path::new("/nonexistent-conformance/inventory/constraints.json"),
            },
            &ClauseInputs {
                spec_dir: Path::new("/nonexistent-conformance/spec"),
                clauses_file: Path::new("/nonexistent-conformance/inventory/clauses.json"),
            },
            Path::new("/nonexistent-conformance/snapshots"),
        )
    }
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dir");
    for entry in std::fs::read_dir(src).expect("read dir").flatten() {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}

/// The violations of one class, or a panic naming everything reported — a diagnostic that
/// says what DID fire is far more useful than "expected V28, found none".
fn of_class<'a>(violations: &'a [Violation], code: &str) -> Vec<&'a Violation> {
    violations.iter().filter(|v| v.code == code).collect()
}

fn obligation_blockers<'a>(cert: &'a Certification, code: &str) -> Vec<&'a str> {
    cert.blocking
        .iter()
        .filter(|b| b.kind == BlockingKind::Obligation && b.code.as_deref() == Some(code))
        .map(|b| b.id.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// The control: the base fixture is clean.
// ---------------------------------------------------------------------------

/// Every assertion below is "the gate fires". This one is "the gate is silent when it
/// should be" — without it, a check that fired unconditionally would pass all of them.
#[test]
fn the_base_fixture_is_clean_and_certifies() {
    let fixture = Fixture::base();
    assert!(
        fixture.violations().is_empty(),
        "the control fixture must produce no obligation violation, got: {:#?}",
        fixture.violations()
    );
    let cert = fixture.certify(TODAY);
    assert!(
        cert.certified,
        "the control fixture must certify, got: {:#?}",
        cert.blocking
    );
}

// ---------------------------------------------------------------------------
// T052 — scenario 1: an undispositioned obligation fails certification.
// ---------------------------------------------------------------------------

/// An obligation with no `odp-` record blocks certification as **V28**, and the message
/// names the obligation, what it is about, and its context — actionable without opening
/// the machine-owned inventory.
#[test]
fn an_undispositioned_obligation_blocks_certification_and_is_named() {
    let fixture = Fixture::with("undispositioned");

    let violations = fixture.violations();
    let v28 = of_class(&violations, "V28");
    assert_eq!(
        v28.len(),
        1,
        "exactly the one deleted record's obligation is undispositioned, got: {violations:#?}"
    );
    assert_eq!(v28[0].record, "obl-cmb-eaaf0756");
    let message = &v28[0].message;
    assert!(
        message.contains("read-configuration"),
        "the message must name the operation: {message}"
    );
    assert!(
        message.contains("sdim-config-source=image")
            && message.contains("sdim-container-state=none"),
        "the message must name the assignment the obligation is about: {message}"
    );

    let cert = fixture.certify(TODAY);
    assert!(!cert.certified, "an undispositioned obligation must block");
    assert_eq!(
        obligation_blockers(&cert, "V28"),
        vec!["obl-cmb-eaaf0756"],
        "the blocker carries class V28 and names the obligation: {:#?}",
        cert.blocking
    );
    assert_eq!(
        cert.obligations.undispositioned, 1,
        "and the summary counts it in its own bucket, never folded into another"
    );
}

/// A behavior obligation states its BEHAVIOR rather than an assignment — the same
/// diagnostic obligation, in the vocabulary of the other kind.
#[test]
fn an_undispositioned_behavior_obligation_names_its_behavior_and_context() {
    let fixture = Fixture::with("new-behavior");
    let violations = fixture.violations();
    let v28 = of_class(&violations, "V28");
    assert_eq!(v28.len(), 1, "got: {violations:#?}");
    assert!(
        v28[0].message.contains("bhv-ports-forward-declared"),
        "the message must name the behavior: {}",
        v28[0].message
    );
    assert!(
        v28[0].message.contains("context: any"),
        "and its context, even when the context is 'everywhere': {}",
        v28[0].message
    );
}

// ---------------------------------------------------------------------------
// T053 — scenario 2: a `gap` disposition blocks.
// ---------------------------------------------------------------------------

/// A `gap` disposition blocks certification — through the `gap-` record it names, which
/// is where gap semantics already live. It produces no V-class violation of its own: an
/// admitted gap is a well-formed judgement, not a defect in the record.
#[test]
fn a_gap_disposition_blocks_certification() {
    let fixture = Fixture::with("gap");
    assert!(
        fixture.violations().is_empty(),
        "an admitted gap is well-formed, not a violation: {:#?}",
        fixture.violations()
    );

    let cert = fixture.certify(TODAY);
    assert!(!cert.certified, "a gap always blocks (FR-020, FR-025)");
    assert!(
        cert.blocking
            .iter()
            .any(|b| b.kind == BlockingKind::Gap && b.id == "gap-fixture-pairwise"),
        "the backing gap record is what blocks: {:#?}",
        cert.blocking
    );
    assert_eq!(
        cert.obligations.gap, 1,
        "and the obligation is counted in the gap bucket"
    );
    assert_eq!(
        cert.obligations.undispositioned, 0,
        "a gap is a decision — it is never counted as undispositioned as well"
    );
}

/// A `gap` disposition naming no declared `gap-` record is **V29**: an admission that
/// nothing backs blocks nothing, so the admission would be free.
#[test]
fn a_gap_disposition_naming_no_gap_record_is_rejected() {
    let fixture = Fixture::with("gap");
    // Delete the gap record, leaving the disposition pointing at nothing.
    std::fs::write(
        fixture.registry_dir.join("gaps.json"),
        "{ \"schemaVersion\": 1, \"records\": [] }\n",
    )
    .expect("rewrite gaps.json");

    let violations = fixture.violations();
    let v29 = of_class(&violations, "V29");
    assert_eq!(v29.len(), 1, "got: {violations:#?}");
    assert!(
        v29[0].message.contains("gap-fixture-pairwise"),
        "the message names the missing gap: {}",
        v29[0].message
    );
}

// ---------------------------------------------------------------------------
// T054 — scenario 3, SC-009: an expired waiver disposition blocks and is named.
// ---------------------------------------------------------------------------

/// The SAME fixture certifies before its waiver expires and blocks after — so what
/// changed is the date, not the registry.
#[test]
fn a_waiver_expiring_before_today_blocks_and_is_named() {
    let fixture = Fixture::base();

    let before = fixture.certify(TODAY);
    assert!(
        before.certified,
        "an unexpired waived disposition must not block: {:#?}",
        before.blocking
    );

    let after = fixture.certify(AFTER_EXPIRY);
    assert!(
        !after.certified,
        "once the waiver expires, nothing stands between its obligation and undispositioned"
    );
    assert_eq!(
        obligation_blockers(&after, "V6"),
        vec!["wvr-readconfig-malformed-jsonc"],
        "the blocker carries class V6 and NAMES THE WAIVER — the record that expired \
         (SC-009): {:#?}",
        after.blocking
    );
}

/// The expiry boundary passes, matching V6 exactly — the two must not disagree by a day
/// about when a waiver dies.
#[test]
fn the_expiry_boundary_passes() {
    let fixture = Fixture::base();
    let cert = fixture.certify("2027-01-19");
    assert!(
        cert.certified,
        "`expires == today` is still valid, as V6 has it: {:#?}",
        cert.blocking
    );
}

// ---------------------------------------------------------------------------
// T055 — scenario 4: unexpired `waived` and `non-testable` do not block, and are
// enumerated.
// ---------------------------------------------------------------------------

/// Non-blocking is not the same as invisible. Both dispositions leave the registry
/// certifiable AND appear in their own buckets, because a count that vanished when it
/// stopped blocking could not be read as backlog.
#[test]
fn unexpired_waived_and_non_testable_do_not_block_and_are_enumerated() {
    let fixture = Fixture::base();
    let cert = fixture.certify(TODAY);

    assert!(
        cert.certified,
        "neither disposition blocks: {:#?}",
        cert.blocking
    );

    let o = &cert.obligations;
    assert_eq!(o.waived, 1, "the waived obligation is enumerated");
    assert_eq!(
        o.non_testable, 9,
        "the non-testable obligations are enumerated"
    );
    assert_eq!(o.covered, 2);
    assert_eq!(o.gap, 0);
    assert_eq!(
        o.inactive_environment, 1,
        "the Podman-only behavior's obligation is enumerated as backlog, not dropped"
    );
    assert_eq!(o.undispositioned, 0);
    assert_eq!(
        o.covered + o.waived + o.non_testable + o.gap + o.inactive_environment + o.undispositioned,
        o.total,
        "the buckets partition the obligation set — none is folded into another (FR-026)"
    );

    // The waiver is also listed among the registry's waivers, exactly as before: the
    // obligation gate adds a bucket, it does not take over waiver reporting.
    assert!(
        cert.waived
            .contains(&"wvr-readconfig-malformed-jsonc".to_string())
    );
}

/// An `inactive-environment` obligation is owed no disposition — the base fixture leaves
/// the Podman-only behavior's obligation undispositioned on purpose, and that is not V28.
#[test]
fn an_inactive_environment_obligation_is_owed_no_disposition() {
    let fixture = Fixture::base();
    assert!(
        of_class(&fixture.violations(), "V28").is_empty(),
        "an obligation outside the active profile owes nobody a decision: {:#?}",
        fixture.violations()
    );
    assert_eq!(fixture.certify(TODAY).obligations.inactive_environment, 1);
}

// ---------------------------------------------------------------------------
// T056 — scenario 5: two dispositions on one obligation fail validation.
// ---------------------------------------------------------------------------

/// Two records for one obligation are two judgements. Validation refuses both rather than
/// resolving to either, and the message names both so the reviewer can see the
/// disagreement they have to settle.
#[test]
fn two_dispositions_on_one_obligation_fail_validation() {
    let fixture = Fixture::with("conflicting");
    let violations = fixture.violations();
    let v28 = of_class(&violations, "V28");
    assert_eq!(v28.len(), 1, "got: {violations:#?}");

    let message = &v28[0].message;
    assert!(message.contains("2 dispositions"), "{message}");
    assert!(
        message.contains("odp-bhv-c0ca20fa") && message.contains("odp-bhv-c0ca20fab"),
        "both records must be named: {message}"
    );

    let cert = fixture.certify(TODAY);
    assert!(!cert.certified, "a conflict must block");
    assert_eq!(obligation_blockers(&cert, "V28"), vec!["obl-bhv-c0ca20fa"]);
    assert_eq!(
        cert.obligations.undispositioned, 1,
        "a conflict resolves to nothing, so the obligation is counted as undecided — \
         never as whichever of the two records happened to sort first"
    );
}

// ---------------------------------------------------------------------------
// T057 — scenario 6, FR-025: a filler rationale is rejected.
// ---------------------------------------------------------------------------

/// A rationale that restates that the obligation is out of scope names no ground, and is
/// indistinguishable from unqueued debt — which is what a gap is for.
///
/// The fixture carries BOTH failure shapes the reused V23 test recognizes, because they
/// fail for different reasons and a check that caught only one would let the other
/// through: a bare restatement short enough to have explained nothing ("Out of scope."),
/// and prose long enough to look considered that names no ground at all.
#[test]
fn a_filler_rationale_is_rejected() {
    let fixture = Fixture::with("filler-rationale");
    let violations = fixture.violations();
    let v29 = of_class(&violations, "V29");
    assert_eq!(v29.len(), 2, "got: {violations:#?}");
    let records: Vec<&str> = v29.iter().map(|v| v.record.as_str()).collect();
    assert_eq!(records, vec!["odp-cmb-e944fc75", "odp-cmb-eaaf0756"]);
    for violation in &v29 {
        assert!(
            violation.message.contains("does not name a ground"),
            "{}",
            violation.message
        );
    }

    let cert = fixture.certify(TODAY);
    assert!(!cert.certified);
    assert_eq!(
        obligation_blockers(&cert, "V29"),
        vec!["odp-cmb-e944fc75", "odp-cmb-eaaf0756"]
    );

    // The control: the SAME disposition kind with a grounded rationale is accepted nine
    // times over in `base`, so this is a judgement about the prose, not about the word.
    assert!(of_class(&Fixture::base().violations(), "V29").is_empty());
}

/// A high-risk triple accepts only `case` or `gap` (FR-015): the triple set is the one
/// place an argument cannot substitute for evidence.
#[test]
fn a_triple_dispositioned_by_argument_is_rejected() {
    let fixture = Fixture::with("triple");
    let violations = fixture.violations();
    let v29 = of_class(&violations, "V29");
    assert_eq!(v29.len(), 1, "got: {violations:#?}");
    assert_eq!(v29[0].record, "odp-cmb-0d8ccfb7");
    assert!(
        v29[0].message.contains("high-risk triple"),
        "{}",
        v29[0].message
    );
    assert!(!fixture.certify(TODAY).certified);
}

/// A `waived` disposition backed by a BLANKET waiver scope is rejected (FR-023) — the
/// analogue of the rule V19 already enforces on an allowed difference. A scope matching
/// every field of a fixture can never self-invalidate.
#[test]
fn a_waived_disposition_backed_by_a_blanket_scope_is_rejected() {
    let fixture = Fixture::with("blanket-waiver");
    let violations = fixture.violations();
    let v29 = of_class(&violations, "V29");
    assert_eq!(v29.len(), 1, "got: {violations:#?}");
    assert!(v29[0].message.contains("blanket"), "{}", v29[0].message);

    // The control: `base` waives an obligation with a SPECIFIC corpus-case scope and is
    // clean, so what is rejected is the breadth, not the disposition.
    assert!(of_class(&Fixture::base().violations(), "V29").is_empty());
}

// ---------------------------------------------------------------------------
// T058 — SC-014: adding a behavior without dispositioning its obligations is rejected.
// ---------------------------------------------------------------------------

/// A behavior added with a case is structurally covered (V5 is satisfied) — and still
/// blocks, because the obligation it generates carries no decision. That is the whole
/// point of the second denominator: behavior-level coverage can be complete while the
/// obligation queue is not.
#[test]
fn adding_a_behavior_without_dispositioning_its_obligation_is_rejected() {
    let fixture = Fixture::with("new-behavior");
    let registry = fixture.registry();

    // The behavior IS covered in the pre-024 sense: a case links to it.
    assert!(
        registry.cases.iter().any(|c| c
            .behaviors
            .iter()
            .any(|b| b == "bhv-ports-forward-declared")),
        "the fixture must give the new behavior a case, or this proves nothing"
    );

    let cert = fixture.certify(TODAY);
    assert!(
        !cert
            .blocking
            .iter()
            .any(|b| b.kind == BlockingKind::Uncovered),
        "the behavior is structurally covered, so nothing may block as `uncovered`: {:#?}",
        cert.blocking
    );
    assert!(!cert.certified, "yet it must still block");
    assert_eq!(
        obligation_blockers(&cert, "V28").len(),
        1,
        "as V28 — an obligation nobody decided: {:#?}",
        cert.blocking
    );
}

// ---------------------------------------------------------------------------
// T059 — FR-024: a disposition whose obligation no longer resolves is stale.
// ---------------------------------------------------------------------------

/// Renaming a dimension value re-hashes every obligation that pins it. The dispositions
/// that judged the old ones are reported **stale** — never quietly dropped — and the new
/// obligations are **undispositioned**, because disposition is never inherited by name.
#[test]
fn a_disposition_whose_obligation_no_longer_resolves_is_stale() {
    let fixture = Fixture::with("stale");
    let violations = fixture.violations();

    let stale = of_class(&violations, "V29");
    assert!(
        !stale.is_empty(),
        "the re-hashed obligations must strand their dispositions: {violations:#?}"
    );
    for violation in &stale {
        assert!(
            violation.message.contains("no longer contains"),
            "a stale disposition must say what it lost: {}",
            violation.message
        );
        assert!(
            violation.record.starts_with("odp-"),
            "and be attributed to the RECORD, not the obligation that vanished: {}",
            violation.record
        );
    }

    let undispositioned = of_class(&violations, "V28");
    assert_eq!(
        undispositioned.len(),
        stale.len(),
        "each stranded judgement corresponds to a NEW obligation nobody has decided — a \
         regenerated obligation that resembles a removed one is not the same obligation"
    );

    let cert = fixture.certify(TODAY);
    assert!(!cert.certified, "stale dispositions block");
    assert!(
        !obligation_blockers(&cert, "V29").is_empty(),
        "and are surfaced as V29 blockers: {:#?}",
        cert.blocking
    );
}

// ---------------------------------------------------------------------------
// The gate is scoped, not global.
// ---------------------------------------------------------------------------

/// A registry that declares no scenario model has not opted into the obligation regime,
/// and owes no dispositions. Without this scoping every pre-024 fixture in the repository
/// would suddenly owe one decision per behavior — a claim that says nothing true about
/// those registries.
#[test]
fn a_registry_with_no_scenario_model_owes_no_dispositions() {
    let registry = Registry::load(&workspace_root().join("fixtures/conformance/valid"))
        .expect("the pre-024 valid fixture loads");
    assert!(
        registry.scenario.is_empty(),
        "this fixture is only meaningful while it declares no scenario model"
    );
    assert!(
        !registry.behaviors.is_empty(),
        "and while it HAS behaviors, which would otherwise generate obligations"
    );
    assert!(
        check_obligation_dispositions(&registry).is_empty(),
        "a registry outside the obligation regime must produce no V28/V29"
    );
}
