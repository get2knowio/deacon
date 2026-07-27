//! PR-gate test (T015; research Decision 8): the REAL authoritative registry under
//! `conformance/registry/` MUST validate cleanly on every PR.
//!
//! This is the hermetic guard that keeps the registry from silently rotting. It runs
//! the full V1–V10 engine (via the shared [`validate_path`] entry) against the seed
//! skeleton (revisions/dimensions/channels/profiles today; behaviors/sources/cases
//! arrive in later phases and must keep this green). No Docker, no network, light
//! filesystem — so it lands in `dev-fast`/CI automatically with no nextest group
//! override (verify: `cargo nextest list -E 'binary(=registry_valid)'`).

use deacon_conformance::load::Registry;
use deacon_conformance::obligation::{
    ObligationInventory, compare as compare_obligations, generate_obligations,
    render as render_obligations,
};
use deacon_conformance::validate::validate_path;
use deacon_conformance::{default_registry_dir, obligations_file_for, workspace_root};

/// A fixed injected "today" so the gate never depends on the wall clock. The seed
/// registry has no waivers, so V6 cannot fire regardless — but pinning the date
/// keeps the test deterministic as waivers are added.
const TODAY: &str = "2026-07-19";

/// The number of case records the registry held when `cases.json` was split into
/// `cases/<area>.json` (024 T007). The split is a mechanical move whose one real risk
/// is silently dropping records — a per-area file that is never read, or a record that
/// falls out during a regroup, would just make the denominator smaller, and a smaller
/// denominator is invisible in every downstream count.
///
/// This constant is therefore a deliberate-edit gate, NOT a floor: raising it is how a
/// PR that genuinely adds cases records that it did so, and lowering it is how a PR that
/// genuinely retires cases records that it did so. An unexplained change to this number
/// is exactly the event the guard exists to surface.
///
/// **88 → 169** (024 US3): the deterministic build-out of the consumer workflow added 81
/// cases, taking every one of the ten operations from the three that had coverage to all
/// ten, and every operation to a case in each of the five input classes and each permitted
/// configuration source. No record was removed.
///
/// **169 → 181** (024 US4): the container-backed error-path tier added 12 cases — nine
/// error-path cases spanning the five later-stage failure points (build ×2, container
/// creation ×2, Feature installation ×2, lifecycle execution ×2, teardown ×1) and three
/// `-direction` spec-expectation twins pinning which side is right where the two
/// implementations disagree. No record was removed.
///
/// **181 → 198** (024 US5): the de-suppression pass added 17 cases for the twelve fields
/// broad normalization used to hide — lifecycle hooks in both forms, chained Feature
/// entrypoints, environment merge precedence, PATH construction, the effective user and its
/// UID/GID, label namespaces, mount source versus mount shape, networks, Compose project
/// resources, and the null/empty/omitted distinction — each a spec-expectation pinning
/// deacon's side and, where the two implementations have room to disagree, a live
/// differential alongside it. No record was removed.
///
/// **198 → 199** (024 T150): re-reviewing the `non-testable` clause classifications left
/// exactly one clause that was neither out of scope nor already covered — the cwd rule for
/// lifecycle hooks — so it became `behavior-mapped` on a new behavior, which arrives with
/// this one spec-expectation case as its evidence. No record was removed.
///
/// **199 → 200** (024 divergence characterization): the live Docker tier's exec/dockerfile/
/// cli-overlay case was carrying two subjects at once — the environment LAYERING, which the
/// two CLIs agree on, and the image's `ENV PATH`, which they do not. The PATH divergence
/// lands in arbitrary stdout TEXT, and a tolerance over the whole of `chan-stdout` is a
/// wildcard by any reading (which is precisely why that channel is excluded from
/// `SCALAR_OBSERVABLE`), so the difference could be neither expressed nor characterized while
/// the two shared one case. Splitting the PATH probe into `case-exec-image-path-overlay`
/// gives each case one subject and scopes the exit-code tolerance to a case whose only exit
/// code IS the probe. No record was removed.
///
/// **200 → 204** (024 T142): the observable-channel floor. `chan-image` had ONE covering
/// case, `chan-filesystem` / `chan-injected-process` / `chan-process-graph` two each, all
/// below the three SC-005 requires — a channel carried by a single case is one authoring
/// mistake away from being unobserved. Four cases close it: a Compose `up` (the declared
/// service image and both declared mounts), a Dockerfile-plus-Feature `up` (the base
/// image's `ENV` and entrypoint surviving the Feature layer, and reaching the process
/// under the configured non-root user), a re-entry `up` against a RUNNING container (the
/// reattached process context, which the metamorphic relationship does not look at), and
/// a multi-file `templates apply` (a scaffolded tree, the template manifest that must not
/// be scaffolded, and a mode). No record was removed.
const MIGRATED_CASE_COUNT: usize = 204;

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

/// 024 T009: the per-area case split (T007) preserves every record, and the loader
/// (T008) still presents them in one deterministic, id-sorted sequence regardless of
/// how the areas are named or split.
#[test]
fn every_migrated_case_survives_the_per_area_split() {
    let registry_dir = default_registry_dir();
    let registry = Registry::load(&registry_dir).unwrap_or_else(|e| {
        panic!(
            "the real registry at {} is unreadable: {e}",
            registry_dir.display()
        )
    });

    let ids: Vec<&str> = registry.cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        MIGRATED_CASE_COUNT,
        "conformance/registry/cases/ must load exactly {MIGRATED_CASE_COUNT} case records; \
         loaded {} — if this change is intentional, update MIGRATED_CASE_COUNT in the same \
         commit and say why. Loaded ids: {ids:#?}",
        ids.len()
    );

    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "the concatenated case set must be id-sorted across per-area files, so aggregate \
         ordering is a property of the data and not of how the areas happen to be split"
    );

    sorted.dedup();
    assert_eq!(
        sorted.len(),
        ids.len(),
        "two case records claim the same id across per-area files"
    );
}

/// 024 T049: the committed obligation inventory byte-matches a fresh regeneration — the
/// hermetic form of `coverage check`, run on every PR.
///
/// Byte equality is the contract (V27), because the committed file is machine-owned: a
/// hand edit and a stale regeneration are indistinguishable, and both must fail. Editing
/// a dimension value or an applicability rule without regenerating is precisely the drift
/// this catches, and it catches it before the reports built on the inventory start
/// disagreeing with the model.
#[test]
fn committed_obligations_match_a_fresh_regeneration() {
    let registry_dir = default_registry_dir();
    let registry = Registry::load(&registry_dir).unwrap_or_else(|e| {
        panic!(
            "the real registry at {} is unreadable: {e}",
            registry_dir.display()
        )
    });

    let regenerated = generate_obligations(&registry).expect("obligation generation succeeds");
    let obligations_file = obligations_file_for(&registry_dir);
    let committed = std::fs::read_to_string(&obligations_file).unwrap_or_else(|e| {
        panic!(
            "the committed obligation inventory {} is unreadable: {e} (run `cargo run -p \
             deacon-conformance -- coverage generate`)",
            obligations_file.display()
        )
    });

    if committed == render_obligations(&regenerated) {
        return;
    }
    let parsed: ObligationInventory = serde_json::from_str(&committed).unwrap_or_else(|e| {
        panic!(
            "{} is out of date AND unparseable: {e}; run `coverage generate`",
            obligations_file.display()
        )
    });
    let drift = compare_obligations(&parsed, &regenerated);
    // An EMPTY semantic diff with differing bytes is not drift at all — the records are
    // identical and only the encoding differs. That is a re-encoded checkout (a Windows
    // CRLF translation, which JSON parses away), and `coverage generate` cannot fix it:
    // regeneration writes LF and the checkout rewrites it right back. Name the real cause
    // rather than sending the reader after phantom drift. `.gitattributes` pins
    // `conformance/obligations/** -text` to prevent it.
    assert!(
        !drift.is_empty(),
        "{} parses to EXACTLY the regenerated records but does not byte-match \
         (committed {} bytes, regenerated {} bytes; committed contains CR: {}). This is a \
         line-ending / encoding difference, NOT obligation drift — `coverage generate` \
         will not fix it. Check that `.gitattributes` marks the path `-text`.",
        obligations_file.display(),
        committed.len(),
        render_obligations(&regenerated).len(),
        committed.contains('\r'),
    );
    panic!(
        "{} does not byte-match a fresh regeneration (V27): first difference {:?}; \
         +{} added, -{} removed, ~{} changed. Run `cargo run -p deacon-conformance -- \
         coverage generate`",
        obligations_file.display(),
        drift.first_difference(),
        drift.added.len(),
        drift.removed.len(),
        drift.changed.len()
    );
}

/// 024 T149 / FR-004a: **exactly one** environment profile is active.
///
/// Both `Coverage::evaluate` and the validator select the active profile with
/// `.find(|p| p.active)` — the FIRST match, silently. With one record in the file that was
/// harmless. It stopped being harmless the moment a second profile was modelled: a
/// contributor "activating" it by setting a second `active: true` gets a result that looks
/// like an activation, reports cleanly, and means nothing, because the first record still
/// wins and the second is never consulted.
///
/// A silent no-op is the one outcome the whole conformance model exists to prevent, so the
/// arity is asserted rather than assumed. Activating a further environment (FR-004b) stays a
/// pure data change — it is a *swap* of which record carries the flag, not an addition.
#[test]
fn exactly_one_environment_profile_is_active() {
    let registry_dir = default_registry_dir();
    let registry = Registry::load(&registry_dir).unwrap_or_else(|e| {
        panic!(
            "the real registry at {} is unreadable: {e}",
            registry_dir.display()
        )
    });

    let active: Vec<&str> = registry
        .profiles
        .iter()
        .filter(|p| p.active)
        .map(|p| p.id.as_str())
        .collect();

    assert_eq!(
        active.len(),
        1,
        "exactly one profile in conformance/registry/profiles.json must be `active` \
         (FR-004a); found {}: {active:?}. Coverage and validation resolve the active \
         profile with `.find(|p| p.active)`, so a second active record is never consulted \
         — the activation would silently do nothing. To activate a different environment, \
         MOVE the flag rather than adding one.",
        active.len()
    );
}
