//! Live metamorphic-relation binary (025-exploratory-parity-discovery, US6).
//!
//! **Selected ONLY by `[profile.discovery]`**, like `discovery_campaign`. Its exclusion
//! from the pull-request lanes is about **stochasticity, not cost**: this tier needs
//! neither the pinned oracle, nor Docker, nor the network (research D12), so it is cheap
//! enough to run anywhere — and FR-055 is absolute regardless. A lane whose result varies
//! run to run cannot be a gate.
//!
//! That cheapness is load-bearing elsewhere, though: it makes this the only complete
//! vertical slice a contributor with no devcontainer CLI installed can develop and test
//! locally, which is why research D12 recommends building it first.
//!
//! ## What these tests are, and are not
//!
//! Each test drives the real `deacon` binary over a real workspace, applies one declared
//! transformation, and checks the relation the registry declares for it. They are **not**
//! unit tests of the transformations — those live beside the transformations in
//! `parity_harness::discovery::metamorphic_run` — and they are not a differential: no
//! oracle is involved, and the two sides being compared are both deacon.
//!
//! ## Lane wiring (T097)
//!
//! Two guards, deliberately at different levels, because they fail in different ways:
//!
//! - [`this_binary_runs_only_under_the_discovery_profile`] is a **runtime** check. It fires
//!   when a lane actually selected this binary, which a config-shaped assertion cannot see.
//! - `discovery_hermetic::live_discovery_binaries_are_selected_by_exactly_one_lane` is the
//!   **config** check, and it names `discovery_metamorphic` explicitly: selected by
//!   `[profile.discovery]`, selected by none of the six pull-request profiles (`default`,
//!   `dev-fast`, `full`, `ci`, `mvp-integration`, `parity`). It lives there rather than
//!   here precisely so it runs in the fast lane — a guard that only runs in the lane it
//!   guards guards nothing.

use std::path::{Path, PathBuf};

use deacon_conformance::discovery::metamorphic::{
    MANDATED_RELATIONS, MetamorphicRelation, RelationEffect,
};
use parity_harness::discovery::metamorphic_run::{
    MetamorphicCandidate, RelationOutcome, Sabotage, evaluate, evaluate_catalogue,
};

/// The environment variable nextest sets to the profile it selected the run under.
const NEXTEST_PROFILE: &str = "NEXTEST_PROFILE";

/// The one profile permitted to select this binary.
const DISCOVERY_PROFILE: &str = "discovery";

/// If this binary is running under a nextest profile at all, that profile must be
/// `discovery`.
///
/// See `discovery_campaign`'s counterpart for why the invariant is asserted from inside
/// the binary as well as against the config: this catches a lane that actually selected
/// the binary, not merely a config that reads as though it would not.
#[test]
fn this_binary_runs_only_under_the_discovery_profile() {
    let Some(profile) = std::env::var_os(NEXTEST_PROFILE) else {
        // Run outside nextest: no profile selected it, so there is no selection to check.
        return;
    };
    let profile = profile.to_string_lossy().into_owned();
    assert_eq!(
        profile, DISCOVERY_PROFILE,
        "the live metamorphic binary was selected by [profile.{profile}]. Only \
         [profile.{DISCOVERY_PROFILE}] may select it — the exclusion is about \
         stochasticity, not cost, so \"this tier is cheap\" is never a reason to admit it \
         to a pull-request lane (FR-055/FR-057). Fix the profile's `default-filter` in \
         .config/nextest.toml."
    );
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The deacon binary under test. Only the test crate can expand this; the harness never
/// guesses a `target/…/deacon` path (the stale-artifact defect of 023 T115).
fn deacon() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_deacon"))
}

/// The committed relation catalogue.
///
/// Loaded from the registry rather than restated here, so these tests exercise the records
/// `validate` polices. A catalogue that stopped loading is a failure, never an empty run:
/// zero relations evaluated and zero relations violated look identical from the outside.
fn catalogue() -> Vec<MetamorphicRelation> {
    let path = deacon_conformance::default_registry_dir().join("metamorphic.json");
    let records = deacon_conformance::discovery::metamorphic::load_metamorphic(&path)
        .unwrap_or_else(|e| panic!("the committed catalogue at {path:?} must load: {e}"));
    assert!(
        !records.is_empty(),
        "the committed catalogue at {path:?} is empty — an empty catalogue makes every \
         relation test pass vacuously"
    );
    records
}

/// The catalogue record for `id`.
fn relation(id: &str) -> MetamorphicRelation {
    catalogue()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("`{id}` is not declared in the committed catalogue"))
}

/// Evaluate one relation honestly and return its outcome.
async fn run(id: &str, root: &Path) -> RelationOutcome {
    let record = relation(id);
    evaluate(&deacon(), root, &record, Sabotage::None)
        .await
        .unwrap_or_else(|e| panic!("`{id}` must be evaluable: {e}"))
}

/// Assert an invariance relation held, rendering every residual difference on failure.
fn assert_holds(outcome: &RelationOutcome) {
    assert!(
        outcome.holds,
        "relation `{}` was violated.\n  transformation: {}\n  effect: {}\n  residual \
         differences ({}):\n{}\n  original workspace: {}\n  transformed workspace: {}",
        outcome.relation,
        outcome.transformation,
        outcome.effect.as_str(),
        outcome.residual.len(),
        render_residual(outcome),
        outcome.original.workspace,
        outcome.transformed.workspace,
    );
}

fn render_residual(outcome: &RelationOutcome) -> String {
    if outcome.residual.is_empty() {
        return "    (none)".to_string();
    }
    outcome
        .residual
        .iter()
        .map(|r| {
            format!(
                "    {} [{}]\n      original    = {}\n      transformed = {}",
                r.path,
                r.kind,
                r.original
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<absent>".to_string()),
                r.transformed
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<absent>".to_string()),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temp workspace root")
}

// ---------------------------------------------------------------------------
// T084 — invariance: formatting, JSONC comments/trailing commas, key order (FR-044)
// ---------------------------------------------------------------------------

/// Reindenting the configuration and rewrapping its whitespace changes no token, so the
/// resolved configuration must be identical. A result that changes here is reading layout.
#[tokio::test]
async fn formatting_is_invariant() {
    let dir = tempdir();
    let outcome = run("mrl-formatting-invariance", dir.path()).await;
    assert_holds(&outcome);
    // The transformation must actually have happened: an identity "transformation" would
    // make the relation hold for a reason that says nothing about deacon.
    assert_ne!(
        outcome.original.input, outcome.transformed.input,
        "the reindented document must differ from the original by more than nothing"
    );
    assert_eq!(
        outcome.original.normalized, outcome.transformed.normalized,
        "an invariance relation that holds compares equal outright"
    );
}

/// JSONC comments and trailing commas are well-formed in the parse dialect and denote no
/// member, so inserting them can change neither whether the document resolves nor what it
/// resolves to. This relation fails in both directions, and both matter.
#[tokio::test]
async fn jsonc_comments_and_trailing_commas_are_invariant() {
    let dir = tempdir();
    let outcome = run("mrl-comment-invariance", dir.path()).await;
    // The `exitCode` channel carries the first direction: a rejected document shows up
    // here, not as a mysterious structural difference.
    assert_eq!(
        outcome.transformed.normalized.get("exitCode"),
        outcome.original.normalized.get("exitCode"),
        "a commented document must resolve exactly as the uncommented one does; residual: \n{}",
        render_residual(&outcome)
    );
    assert_holds(&outcome);
    assert!(
        outcome
            .transformed
            .input
            .files()
            .values()
            .any(|f| f.contains("// injected")),
        "the transformed fixture must genuinely carry comments"
    );
}

/// Permuting the members of an unordered map cannot change the value the document denotes,
/// so the resolved configuration must be identical.
#[tokio::test]
async fn key_order_within_unordered_maps_is_invariant() {
    let dir = tempdir();
    let outcome = run("mrl-key-order-invariance", dir.path()).await;
    assert_holds(&outcome);
    assert_ne!(
        outcome.original.input, outcome.transformed.input,
        "the member order must actually have been permuted"
    );
}

// ---------------------------------------------------------------------------
// T085 — path relocation compares modulo the declared tokenization (FR-046)
// ---------------------------------------------------------------------------

/// Relocating the identical workspace to a different absolute path must leave the result
/// equal **modulo the declared path tokenization**, and any residual the tokenization does
/// not account for is reported rather than tolerated.
///
/// That residual is the interesting output, and it needs careful triage: a leaked absolute
/// path the tokenizer missed is as likely to be a `normalizer-defect` as a
/// `deacon-regression`, and misfiling it as the latter sends someone to fix code that is
/// correct. The failure message therefore names both possibilities rather than asserting
/// one.
#[tokio::test]
async fn path_relocation_is_invariant_modulo_the_declared_tokenization() {
    let dir = tempdir();
    let outcome = run("mrl-path-relocation", dir.path()).await;

    // The two sides really were at different absolute paths, with the same basename — so a
    // difference is relocation and not a renamed directory.
    assert_ne!(outcome.original.workspace, outcome.transformed.workspace);
    assert_eq!(
        Path::new(&outcome.original.workspace).file_name(),
        Path::new(&outcome.transformed.workspace).file_name(),
    );

    // The RAW evidence still carries the two distinct host paths; only the normalized
    // evidence is tokenized. Raw and normalized are held separately for exactly this
    // reason — conflating them would hide what the tokenization did.
    assert_ne!(
        outcome.original.raw, outcome.transformed.raw,
        "the raw evidence must still carry the two distinct absolute paths, or the \
         relocation did not happen and the relation holds vacuously"
    );

    assert!(
        outcome.holds,
        "relation `{}` left a residual the declared path tokenization did not account \
         for.\n{}\nTriage carefully: a leaked absolute path is as likely to be a \
         normalizer-defect (the token map missed a form deacon emits) as a \
         deacon-regression (the result genuinely depends on where the workspace sits). \
         Filing it as the latter sends someone to fix code that is correct.",
        outcome.relation,
        render_residual(&outcome),
    );

    // And the tokenization actually did something: the tokenized host path is gone from
    // both normalized documents.
    for side in [&outcome.original, &outcome.transformed] {
        let rendered = side.normalized.to_string();
        assert!(
            !rendered.contains(&side.workspace),
            "the normalized evidence still contains the raw workspace path {}; the \
             tokenization did not run, so this relation would compare two documents that \
             cannot be equal",
            side.workspace
        );
        assert!(
            rendered.contains("<WORKSPACE>"),
            "the normalized evidence carries no `<WORKSPACE>` token, so the fixture puts \
             no host path into the result and the relation is vacuous"
        );
    }
}

// ---------------------------------------------------------------------------
// T086 — lifecycle equivalence across the permitted forms
// ---------------------------------------------------------------------------

/// The bare form and the single-entry named-object form denote the same command, so both
/// must resolve and switching between them must change nothing outside the lifecycle
/// property itself.
#[tokio::test]
async fn equivalent_lifecycle_command_forms_resolve_equivalently() {
    let dir = tempdir();
    let outcome = run("mrl-lifecycle-equivalence", dir.path()).await;

    // Both permitted forms are accepted — the `chan-exit-code` half of the assertion.
    for side in [&outcome.original, &outcome.transformed] {
        assert_eq!(
            side.normalized.get("exitCode"),
            Some(&serde_json::json!(0)),
            "every permitted lifecycle command form must resolve; workspace {}",
            side.workspace
        );
    }

    assert_holds(&outcome);

    // The difference the relation accounts for is the transformed site and nothing else.
    // Asserting it is present matters as much as asserting the residual is empty: an
    // accounting that absorbed nothing would mean the rewrite never reached the output,
    // and the relation would hold because nothing happened.
    assert!(
        outcome.accounted.iter().any(|r| r
            .path
            .starts_with("structuredOutput.configuration.postCreateCommand")),
        "the rewritten lifecycle property must show up in the output; accounted: {:?}",
        outcome.accounted
    );
    assert_eq!(
        outcome.transformed.raw["structuredOutput"]["configuration"]["postCreateCommand"],
        serde_json::json!({ "solo": "echo hi" }),
        "the object form must be echoed as authored"
    );
}

// ---------------------------------------------------------------------------
// T087 — sensitivity: permuting a declaration-ordered collection MUST change the result
// ---------------------------------------------------------------------------

/// Reversing a declaration-ordered collection describes a different configuration, so the
/// result **must** change. A failure to change is a finding, not a pass.
///
/// This is the assertion the differential structurally cannot make. If deacon and the
/// reference both canonicalized this list, the two sides would agree and the differential
/// would be clean — the defect invisible precisely because both implementations share it.
#[tokio::test]
async fn permuting_a_declaration_ordered_collection_must_change_the_result() {
    let dir = tempdir();
    let outcome = run("mrl-declaration-order-sensitivity", dir.path()).await;
    assert_eq!(outcome.effect, RelationEffect::Sensitivity);

    assert!(
        outcome.holds,
        "relation `{}` was violated: reversing a declaration-ordered collection left the \
         resolved configuration unchanged. The cited clause says the order of this array \
         matters, so an unchanged result means the order was discarded — a defect the \
         differential cannot see, because a reference that discards it too would agree.\n\
         original    = {}\n transformed = {}",
        outcome.relation, outcome.original.normalized, outcome.transformed.normalized,
    );

    // The change is where the clause says it is, and it is a reordering rather than a loss.
    let changed = outcome
        .residual
        .iter()
        .chain(outcome.accounted.iter())
        .find(|r| r.path.contains("dockerComposeFile"))
        .unwrap_or_else(|| {
            panic!(
                "the result changed, but not at `dockerComposeFile` — the relation would be \
                 satisfied by an unrelated difference. residual: {:?} accounted: {:?}",
                outcome.residual, outcome.accounted
            )
        });
    assert!(
        changed.original.is_some() && changed.transformed.is_some(),
        "the overlay list must be present on both sides, reordered rather than dropped: {changed:?}"
    );
}

// ---------------------------------------------------------------------------
// T090 — zero relations are inert (SC-011)
// ---------------------------------------------------------------------------

/// Deliberately breaking each declared relation causes **exactly that relation** to fail
/// and be named.
///
/// The break is applied to the **input**, never to an observation. Perturbing what the
/// comparison returns would make a comparison that ignores its arguments look alive — the
/// same defect 024's regression harness forbids by sealing its injection boundary. The two
/// breaks used here are the two ways a relation can genuinely be violated: an invariance
/// relation is given a genuinely different input, and a sensitivity relation has its
/// transformation taken away.
///
/// A relation that still holds under its own break is **inert**: it reports a pass that no
/// defect could turn into a failure, which is worse than having no relation at all, because
/// the pass is trusted.
#[tokio::test]
async fn breaking_each_relation_fails_exactly_that_relation() {
    let dir = tempdir();
    let records = catalogue();

    // Every mandated family is present, so "each relation" is the declared floor and not
    // whatever happens to be in the file.
    for family in MANDATED_RELATIONS {
        assert!(
            records.iter().any(|r| &r.id.as_str() == family),
            "mandated relation `{family}` is absent from the catalogue, so this test would \
             silently not cover it"
        );
    }

    for record in &records {
        let root = dir.path().join(format!("sabotage-{}", record.id));

        // Sanity: the relation holds when it is not broken. Without this the next
        // assertion would also pass for a relation that fails unconditionally.
        let clean = evaluate(&deacon(), &root.join("clean"), record, Sabotage::None)
            .await
            .unwrap_or_else(|e| panic!("`{}` must be evaluable: {e}", record.id));
        assert!(
            clean.holds,
            "`{}` does not hold even unbroken, so breaking it proves nothing: {}",
            record.id,
            render_residual(&clean)
        );

        let broken = evaluate(&deacon(), &root.join("broken"), record, Sabotage::Break)
            .await
            .unwrap_or_else(|e| panic!("`{}` must be evaluable when broken: {e}", record.id));
        assert!(
            !broken.holds,
            "`{}` still holds after being deliberately broken — it is INERT (SC-011). An \
             inert relation reports a pass no defect could turn into a failure, which is \
             worse than no relation at all, because the pass is trusted. Effect: {}.",
            record.id,
            record.effect.as_str(),
        );

        // The failure names the relation, and its candidate names it too.
        let candidate = broken
            .candidate()
            .unwrap_or_else(|| panic!("`{}` failed but produced no candidate", record.id));
        assert_eq!(candidate.relation, record.id);
    }
}

/// Breaking one relation must not make the others fail: a break that turned the whole tier
/// red would prove the machinery reacts to *something*, not that each relation observes its
/// own transformation.
#[tokio::test]
async fn a_break_is_confined_to_the_relation_it_targets() {
    let dir = tempdir();
    let records = catalogue();
    let target = &records[0];

    let broken = evaluate(
        &deacon(),
        &dir.path().join("target"),
        target,
        Sabotage::Break,
    )
    .await
    .expect("evaluable");
    assert!(!broken.holds, "the targeted relation must fail");

    for other in records.iter().skip(1) {
        let outcome = evaluate(
            &deacon(),
            &dir.path().join(format!("other-{}", other.id)),
            other,
            Sabotage::None,
        )
        .await
        .unwrap_or_else(|e| panic!("`{}` must be evaluable: {e}", other.id));
        assert!(
            outcome.holds,
            "`{}` failed while `{}` was the one being broken: {}",
            other.id,
            target.id,
            render_residual(&outcome)
        );
    }
}

// ---------------------------------------------------------------------------
// T127 — the failure candidate (FR-047)
// ---------------------------------------------------------------------------

/// A metamorphic failure produces a candidate naming the relation, the transformation
/// applied, **both** inputs, and **both** normalized outputs.
///
/// Every one of the four is required, and the reason is the reviewer: a candidate that
/// names the relation but not the transformation cannot be reproduced, and one that carries
/// a verdict but not both sides' evidence is a bug report with the evidence left behind.
#[tokio::test]
async fn a_metamorphic_failure_emits_a_reviewable_candidate() {
    let dir = tempdir();
    let record = relation("mrl-formatting-invariance");
    let broken = evaluate(&deacon(), dir.path(), &record, Sabotage::Break)
        .await
        .expect("evaluable");
    assert!(!broken.holds, "the broken relation must fail");

    let candidate: MetamorphicCandidate = broken.candidate().expect("a failure yields a candidate");

    // 1. The relation.
    assert_eq!(candidate.relation, "mrl-formatting-invariance");
    // 2. The transformation, worded as the catalogue words it — what a reviewer reproduces.
    assert_eq!(candidate.transformation, record.transformation);
    assert!(!candidate.transformation.trim().is_empty());
    // 3. Both inputs, whole and distinct.
    assert!(
        candidate
            .original_input
            .files()
            .contains_key(".devcontainer/devcontainer.json"),
        "the original input must be the whole workspace tree"
    );
    assert!(
        candidate
            .transformed_input
            .files()
            .contains_key(".devcontainer/devcontainer.json")
    );
    assert_ne!(candidate.original_input, candidate.transformed_input);
    // 4. Both normalized outputs, and they differ — which is what the candidate claims.
    assert_ne!(
        candidate.original_normalized,
        candidate.transformed_normalized
    );

    // The residual is the reviewable difference, and the derived signature is computed by
    // the same function the differential uses — so a metamorphic finding and a differential
    // finding at the same path deduplicate against each other.
    assert!(
        !candidate.residual.is_empty(),
        "an invariance failure must name at least one residual difference"
    );
    let signatures = candidate.signatures("chan-structured-output");
    assert_eq!(signatures.len(), candidate.residual.len());
    for signature in &signatures {
        assert_eq!(
            signature.derived_id(),
            signature.id,
            "the signature id must recompute from its own substance"
        );
        assert!(signature.finding_id().starts_with("fnd-"));
    }

    // A candidate round-trips through strict JSON, so it can be written to the queue and
    // read back without losing the evidence.
    let raw = serde_json::to_string(&candidate).expect("serializes");
    let back: MetamorphicCandidate = serde_json::from_str(&raw).expect("round-trips");
    assert_eq!(back, candidate);
}

// ---------------------------------------------------------------------------
// The tier entry point (the seam T096 will call from the campaign driver)
// ---------------------------------------------------------------------------

/// The whole committed catalogue evaluates in one call, in catalogue order, with no
/// oracle, no Docker, and no network (FR-048, research D12).
///
/// This is the entry point the campaign driver will invoke, and it is tested here rather
/// than left to that integration: an untested entry point is where an integration breaks,
/// and its two failure modes — a relation with no transformation, and a relation silently
/// skipped — are precisely the ones that would surface as a quiet green tier.
#[tokio::test]
async fn the_whole_catalogue_evaluates_with_no_external_prerequisite() {
    let dir = tempdir();
    let records = catalogue();
    let outcomes = evaluate_catalogue(&deacon(), dir.path(), &records, Sabotage::None)
        .await
        .expect("the committed catalogue must be evaluable end to end");

    assert_eq!(
        outcomes.len(),
        records.len(),
        "every declared relation must be evaluated — a skipped relation reports nothing, \
         and reporting nothing is indistinguishable from holding"
    );
    for (outcome, record) in outcomes.iter().zip(records.iter()) {
        assert_eq!(outcome.relation, record.id, "catalogue order is preserved");
        assert_holds(outcome);
    }

    // Both effects are exercised, so a tier that is green is green on both kinds of
    // assertion rather than on six invariances and a relation nobody ran.
    assert!(
        outcomes
            .iter()
            .any(|o| o.effect == RelationEffect::Invariance)
    );
    assert!(
        outcomes
            .iter()
            .any(|o| o.effect == RelationEffect::Sensitivity)
    );
}

/// A relation that holds emits no candidate. A tier that produced a candidate per
/// evaluation would drown the queue in non-findings and make the count meaningless.
#[tokio::test]
async fn a_relation_that_holds_emits_no_candidate() {
    let dir = tempdir();
    let outcome = run("mrl-key-order-invariance", dir.path()).await;
    assert!(outcome.holds);
    assert!(outcome.candidate().is_none());
}
