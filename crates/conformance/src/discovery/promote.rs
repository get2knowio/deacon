//! Review-only promotion: the skeletons `discovery scaffold` prints, and the rules that
//! keep promotion a **human** act (025-exploratory-parity-discovery, US5,
//! FR-036 – FR-042, contracts/discovery-cli.md § `discovery scaffold`).
//!
//! ## Nothing here writes anything
//!
//! Every function in this module is a pure `Finding → JSON` transformation. There is no
//! path, no file handle, and no writer — not as a discipline someone must remember, but
//! because the module never takes one. `discovery scaffold` prints the result to
//! **stdout** and stops; the reviewer copies what they judge correct into the registry by
//! hand. That is what makes FR-036 hold: a stochastic process cannot author the record it
//! is tested against, because there is no code path from a finding to a registry write.
//!
//! ## Why every field carries a sentinel the loader rejects
//!
//! A skeleton whose fields were plausible defaults would invite committing it unread, and
//! the one thing a finding cannot tell you is the thing a behavior record must state: a
//! difference says *what* differs, never whether deacon is wrong, the reference is wrong,
//! or the specification is silent. So each field a human must decide carries
//! [`UNREVIEWED`], which the registry loader rejects
//! ([`crate::residual::UNREVIEWED_SENTINEL`]) — the same discipline `inventory scaffold`,
//! `clause scaffold`, `coverage scaffold`, and `migration scaffold` already use.
//!
//! ## The one rule that is a rule rather than a sentinel (FR-041)
//!
//! A tolerance must be **scoped**. [`reject_blanket_observable_path`] is the single
//! definition of that rule: an `observablePath` that names a bare channel is a global
//! ignore list wearing a waiver id, and the registry rejects one at load (V19). Emitting
//! one here and letting the loader catch it later would be worse than refusing, because
//! the intermediate state is a reviewer holding a document that looks authored.

use serde_json::{Value, json};

use crate::model::{CHAN_EXIT_CODE, Expect, Scope};
use crate::residual::UNREVIEWED_SENTINEL;

use super::queue::{Finding, ObservedValues};
use super::signature::Signature;

/// The sentinel every field a human must decide carries.
///
/// Re-exported from [`crate::residual`] rather than redeclared: two spellings of "not yet
/// reviewed" is exactly the drift that would let one scaffold's output slip past the
/// loader rejection the other relies on.
pub const UNREVIEWED: &str = UNREVIEWED_SENTINEL;

/// The three disposition axes a behavior record must carry (FR-037/FR-038).
///
/// Named here as the *promotion pre-flight's* view of them and cross-checked against
/// [`crate::model::BehaviorUnit`]'s own serialization by this module's
/// `the_axis_names_match_the_behavior_record` test, so renaming an axis on the model fails
/// this module rather than silently making the pre-flight check a field that no longer
/// exists.
pub const BEHAVIOR_DISPOSITION_AXES: [&str; 3] = ["spec", "reference", "decision"];

/// Why a promotion skeleton could not be produced, or why a proposed promotion is not yet
/// a promotion.
///
/// Every variant names the *remedy*, not only the rule: a reviewer meeting one of these
/// is mid-task, and "what is missing" is the only useful thing to say.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromotionError {
    /// FR-035: `normalizer-defect` / `fixture-defect` describe a defect in the discovery
    /// or comparison machinery, not a behavior of either implementation.
    #[error(
        "finding `{finding}` is classified `{classification}`, which is not promotable \
         (FR-035): it describes a defect in the discovery machinery, not a behavior of \
         either implementation. Remedy: fix the normalizer or the generator instead — no \
         registry case can legitimately carry it."
    )]
    NonPromotable {
        finding: String,
        classification: &'static str,
    },

    /// FR-041: a tolerance whose `observablePath` names a bare channel tolerates
    /// everything on that channel forever.
    #[error(
        "tolerance for `{finding}` would be UNSCOPED: `{path}` names a bare channel, which \
         is a global ignore list wearing a waiver id. Remedy: scope it to the observable \
         path the difference actually occurs at — the registry rejects a bare-channel \
         scope at load (V19), and a blanket tolerance silences differences nobody reviewed."
    )]
    UnscopedTolerance { finding: String, path: String },

    /// FR-037: the promotion names no behavior, or still carries the scaffold sentinel.
    #[error(
        "promotion of `{finding}` carries no behavior identity ({detail}). Remedy: name an \
         existing `bhv-` record, or author a new one with all three disposition axes — a \
         promotion with no stable identity cannot be found again, which is the whole point \
         of promoting it."
    )]
    MissingBehaviorIdentity { finding: String, detail: String },

    /// FR-038: a disposition axis is absent, blank, or still the scaffold sentinel.
    #[error(
        "promotion of `{finding}` has no `{axis}` disposition ({detail}). Remedy: state it. \
         A finding tells you what DIFFERS; it never tells you whether deacon is wrong, the \
         reference is wrong, or the specification is silent — that judgement is the review, \
         and a record missing it claims an evidence-backed status nobody established."
    )]
    MissingDisposition {
        finding: String,
        axis: &'static str,
        detail: String,
    },

    /// FR-042 / **D3**, checked *before* the write rather than only after it.
    #[error(
        "promotion of `{finding}` names case `{case}`, which is not declared in the \
         registry. Remedy: commit the case first, then point `promotedTo` at its real id — \
         a promotion the registry cannot back reads as covered while nothing executes it."
    )]
    UnresolvedCase { finding: String, case: String },
}

/// Reject an `observablePath` that is not scoped **within** a channel (FR-041, V19).
///
/// The single definition of the rule, called by [`observable_path`] and driven directly by
/// the guard that asserts a blanket scope is refused. A second copy would be the one that
/// eventually accepted a bare channel.
///
/// Scoped means: a channel id, a `.`, and a non-empty remainder. `chan-structured-output`
/// is refused; `chan-structured-output.configuration` is accepted, and it legitimately
/// covers everything beneath it — the harness's tolerance index matches a scoped path plus
/// its subtree, which is a decision a reviewer made about one field, not a standing licence
/// over a channel.
pub fn reject_blanket_observable_path(finding: &str, path: &str) -> Result<(), PromotionError> {
    let scoped = path
        .split_once('.')
        .is_some_and(|(channel, rest)| !channel.trim().is_empty() && !rest.trim().is_empty());
    if scoped {
        return Ok(());
    }
    Err(PromotionError::UnscopedTolerance {
        finding: finding.to_string(),
        path: path.to_string(),
    })
}

/// The scoped `observablePath` for a signature: `<channel>.<path>`.
///
/// Fails when the signature carries no observable path at all, which would produce a bare
/// channel — the blanket tolerance FR-041 forbids.
pub fn observable_path(finding: &str, signature: &Signature) -> Result<String, PromotionError> {
    let path = format!("{}.{}", signature.channel, signature.path);
    reject_blanket_observable_path(finding, &path)?;
    Ok(path)
}

/// Refuse a finding whose classification can never lead to a registry record (FR-035).
fn require_promotable(finding: &Finding) -> Result<(), PromotionError> {
    match finding.classification {
        Some(classification) if !classification.is_promotable() => {
            Err(PromotionError::NonPromotable {
                finding: finding.id.clone(),
                classification: classification.as_str(),
            })
        }
        _ => Ok(()),
    }
}

/// The behavior + case + fixture skeleton `discovery scaffold` prints (FR-037/FR-039).
///
/// The **fixture** is the one part that is not a sentinel: it is the witness's own reduced
/// input, verbatim. That asymmetry is the point — the machine knows what input reproduces
/// the difference and the human does not, whereas the human knows what the difference
/// *means* and the machine does not. Filling in the half a finding can actually support,
/// and refusing to guess the other half, is what keeps the reviewer's job deciding rather
/// than verifying.
pub fn promotion_skeleton(finding: &Finding) -> Result<Value, PromotionError> {
    require_promotable(finding)?;
    let signature = &finding.signature;
    // A finding always carries at least one witness: the strict loader rejects an empty
    // list and **D1** rejects a programmatically-built one, so the first is always there.
    let Some(witness) = finding.witnesses.first() else {
        return Err(PromotionError::MissingBehaviorIdentity {
            finding: finding.id.clone(),
            detail: "the finding carries no witness, so there is no input to promote".to_string(),
        });
    };

    Ok(json!({
        "behavior": {
            "id": format!("bhv-{UNREVIEWED}"),
            "summary": UNREVIEWED,
            "spec": UNREVIEWED,
            "reference": UNREVIEWED,
            "decision": UNREVIEWED,
            "notes": format!(
                "Promoted from discovery finding {} (signature {}, channel {}, path {}, \
                 {} / {}).",
                finding.id,
                signature.id,
                signature.channel,
                signature.path,
                signature.kind.as_str(),
                signature.value_shape_class.as_str(),
            ),
        },
        "case": {
            "id": format!("case-{UNREVIEWED}"),
            "behaviors": [format!("bhv-{UNREVIEWED}")],
            "oracleType": UNREVIEWED,
            "scenarioContext": UNREVIEWED,
            "fixtures": [format!("fx-{UNREVIEWED}")],
        },
        "fixture": {
            "id": format!("fx-{UNREVIEWED}"),
            "minimalInput": witness.minimal_input,
            "isMinimal": witness.is_minimal,
            "mutationOperators": witness.mutation_operators,
        },
    }))
}

/// The scoped waiver + the `allowedDifferences` entry that references it, for a finding a
/// reviewer chooses to **tolerate** rather than fix (FR-041, `--tolerate`).
///
/// Two records rather than one, because a tolerance in this project is two records: the
/// `wvr-` carries the rationale and the expiry that make it self-invalidating, and the
/// case's scoped `allowedDifferences` entry is what the runner consults. Emitting only the
/// waiver would leave a reviewer with a record nothing reads; emitting only the allowed
/// difference would leave a tolerance with no expiry and no rationale — an unbacked
/// silence, which is the shape V19 exists to refuse.
///
/// `expect` is the one derived field: it restates **what was observed**, which the witness
/// already carries, so the reviewer is not asked to re-read the evidence to retype it.
/// Everything a reviewer must *decide* — the behavior, the rationale, the expiry, the
/// binary/fixture the scope names — is a sentinel.
pub fn tolerance_skeleton(finding: &Finding) -> Result<Value, PromotionError> {
    require_promotable(finding)?;
    let observable_path = observable_path(&finding.id, &finding.signature)?;
    let waiver_id = format!("wvr-{UNREVIEWED}");
    let behavior_id = format!("bhv-{UNREVIEWED}");

    // Scoped to the observable path the difference occurs at. The harness's tolerance
    // index reads `field` and matches it against a signature's path, so this is the field
    // that makes the waiver do anything at all — and it is the one field here taken from
    // the evidence rather than left to the reviewer, for the same reason `expect` is.
    let scope = Scope::StateField {
        binary: UNREVIEWED.to_string(),
        fixture: UNREVIEWED.to_string(),
        field: finding.signature.path.clone(),
    };

    Ok(json!({
        "waiver": {
            "id": waiver_id,
            "behaviors": [behavior_id],
            "scope": scope,
            "expect": observed_expectation(finding),
            "rationale": UNREVIEWED,
            "added": UNREVIEWED,
            "expires": UNREVIEWED,
        },
        "allowedDifference": {
            "behavior": behavior_id,
            // Never empty: an empty context reads as "everywhere", and a tolerance whose
            // validity nobody bounded is the blanket tolerance in a different field.
            "context": [UNREVIEWED],
            "observablePath": observable_path,
            "rationale": UNREVIEWED,
            "waiverId": waiver_id,
        },
        "note": format!(
            "Tolerating finding {} means recording that this difference is ACCEPTABLE, not \
             that it stopped mattering. The waiver is self-invalidating: it fails as stale \
             the moment the difference stops reproducing. Fill in `rationale`, `expires`, \
             the behavior, and the binary/fixture the scope names — the `{UNREVIEWED}` \
             sentinels are rejected by the registry loader, so this cannot be committed \
             unedited.",
            finding.id
        ),
    }))
}

/// The `expect` a waiver would characterize, read from the finding's first witness.
///
/// An accept/reject divergence is a **strictness** claim and gets the directional variant;
/// everything else is the concrete value difference. An observed absence renders as JSON
/// `null` — the same conflation [`ObservedValues`] already documents, harmless here
/// because the signature's own value-shape class carries the presence distinction and the
/// reviewer is editing this record anyway.
fn observed_expectation(finding: &Finding) -> Expect {
    let observed = finding
        .witnesses
        .first()
        .map(|w| w.observed_values.clone())
        .unwrap_or_default();
    let ObservedValues { deacon, reference } = observed;

    if finding.signature.channel == CHAN_EXIT_CODE {
        match deacon.as_ref().and_then(Value::as_str) {
            Some("rejected") => return Expect::DeaconStricter { signal: None },
            Some("accepted") => return Expect::ReferenceStricter { signal: None },
            _ => {}
        }
    }
    Expect::FieldDivergence {
        ours: deacon.unwrap_or(Value::Null),
        reference: reference.unwrap_or(Value::Null),
    }
}

/// Pre-flight a **proposed** promotion: the behavior record a reviewer authored plus the
/// case id the finding would name (FR-037/FR-038/FR-042).
///
/// Reports **every** problem in one pass rather than the first, matching `validate` and
/// `discovery check`: a reviewer fixing a batch should not have to rediscover the next
/// objection by re-running.
///
/// This runs *before* the write; **D3** runs over committed data afterwards. Both exist on
/// purpose: the pre-flight is what tells a reviewer what is missing while they can still
/// act on it cheaply, and D3 is what keeps the claim true afterwards — a case deleted or
/// renamed later produces the same unresolvable promotion, and nothing in the registry can
/// notice, because the registry never reads the queue.
///
/// A field that is absent, blank, or still carrying [`UNREVIEWED`] is treated identically:
/// all three mean "nobody decided this", and distinguishing them would only let a
/// scaffolded record slip through by having been *printed* rather than left out.
pub fn validate_promotion(
    finding: &Finding,
    behavior: &Value,
    case_id: Option<&str>,
    declared_cases: &[&str],
) -> Vec<PromotionError> {
    let mut errors = Vec::new();

    if let Err(e) = require_promotable(finding) {
        errors.push(e);
    }

    match decided(behavior, "id") {
        Ok(id) if id.starts_with("bhv-") => {}
        Ok(id) => errors.push(PromotionError::MissingBehaviorIdentity {
            finding: finding.id.clone(),
            detail: format!("`id` is `{id}`, which is not a `bhv-` record id"),
        }),
        Err(detail) => errors.push(PromotionError::MissingBehaviorIdentity {
            finding: finding.id.clone(),
            detail,
        }),
    }

    for axis in BEHAVIOR_DISPOSITION_AXES {
        if let Err(detail) = decided(behavior, axis) {
            errors.push(PromotionError::MissingDisposition {
                finding: finding.id.clone(),
                axis,
                detail,
            });
        }
    }

    match case_id {
        None => errors.push(PromotionError::UnresolvedCase {
            finding: finding.id.clone(),
            case: "<none>".to_string(),
        }),
        Some(case) if !declared_cases.contains(&case) => {
            errors.push(PromotionError::UnresolvedCase {
                finding: finding.id.clone(),
                case: case.to_string(),
            });
        }
        Some(_) => {}
    }

    errors
}

/// The value of `field` when a human actually decided it, else why it does not count.
fn decided<'a>(record: &'a Value, field: &str) -> Result<&'a str, String> {
    let Some(value) = record.get(field) else {
        return Err(format!("`{field}` is absent"));
    };
    let Some(text) = value.as_str() else {
        return Err(format!(
            "`{field}` is {value}, which is not a decided string"
        ));
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(format!("`{field}` is blank"));
    }
    // `contains`, not `==`: the scaffold prefixes ids with their record kind
    // (`bhv-UNREVIEWED`), so an equality check would let the one field that must be a
    // stable IDENTITY pass by virtue of having been decorated.
    if trimmed.contains(UNREVIEWED) {
        return Err(format!(
            "`{field}` is `{trimmed}`, which still carries the `{UNREVIEWED}` scaffold \
             sentinel"
        ));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::queue::{Classification, FindingState, Witness};
    use crate::discovery::signature::{Divergence, DivergenceKind};
    use crate::model::CHAN_STRUCTURED_OUTPUT;

    fn signature(channel: &str, path: &str) -> Signature {
        let d = json!("vscode");
        let r = json!("root");
        Signature::derive(
            channel,
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: Some(&d),
                reference: Some(&r),
            },
        )
    }

    fn finding(signature: Signature) -> Finding {
        let campaign = "cmp-11111111";
        let witness = Witness {
            id: Witness::derived_id(campaign, "cnd-11111111"),
            campaign_id: campaign.to_string(),
            candidate_id: "cnd-11111111".to_string(),
            minimal_input: json!({ "image": "alpine:3.18" }),
            is_minimal: true,
            reduction_steps: vec!["drop-optional-key".to_string()],
            observed_values: ObservedValues {
                deacon: Some(json!("vscode")),
                reference: Some(json!("root")),
            },
            mutation_operators: vec!["mop-wrong-type".to_string()],
        };
        Finding::newly_admitted(signature, witness, campaign)
    }

    #[test]
    fn the_promotion_skeleton_carries_the_sentinel_everywhere_a_human_must_decide() {
        let f = finding(signature(
            CHAN_STRUCTURED_OUTPUT,
            "configuration.remoteUser",
        ));
        let doc = promotion_skeleton(&f).expect("an untriaged finding scaffolds");
        for axis in BEHAVIOR_DISPOSITION_AXES {
            assert_eq!(
                doc["behavior"][axis],
                json!(UNREVIEWED),
                "`{axis}` must be a sentinel: a finding says what DIFFERS, never whose \
                 behavior is right"
            );
        }
        assert_eq!(doc["behavior"]["id"], json!("bhv-UNREVIEWED"));
        assert_eq!(doc["case"]["scenarioContext"], json!(UNREVIEWED));
        // The one part taken from the evidence: the input that reproduces it.
        assert_eq!(
            doc["fixture"]["minimalInput"],
            json!({"image":"alpine:3.18"})
        );
        assert_eq!(doc["fixture"]["isMinimal"], json!(true));
    }

    #[test]
    fn a_non_promotable_classification_is_refused_by_both_skeletons() {
        for non_promotable in [
            Classification::NormalizerDefect,
            Classification::FixtureDefect,
        ] {
            let mut f = finding(signature(CHAN_STRUCTURED_OUTPUT, "configuration.image"));
            f.state = FindingState::Triaged;
            f.classification = Some(non_promotable);
            for result in [promotion_skeleton(&f), tolerance_skeleton(&f)] {
                assert!(
                    matches!(result, Err(PromotionError::NonPromotable { .. })),
                    "{} must be refused by every promotion path (FR-035)",
                    non_promotable.as_str()
                );
            }
        }
    }

    #[test]
    fn a_tolerance_is_scoped_within_its_channel_never_to_the_channel() {
        let f = finding(signature(
            CHAN_STRUCTURED_OUTPUT,
            "configuration.remoteUser",
        ));
        let doc = tolerance_skeleton(&f).expect("scaffolds");
        assert_eq!(
            doc["allowedDifference"]["observablePath"],
            json!("chan-structured-output.configuration.remoteUser")
        );
        assert_eq!(doc["allowedDifference"]["waiverId"], doc["waiver"]["id"]);
        assert_eq!(
            doc["waiver"]["scope"]["field"],
            json!("configuration.remoteUser"),
            "the scope names the observable path, which is what makes the waiver do \
             anything at all"
        );
        for sentinel in [
            &doc["waiver"]["rationale"],
            &doc["waiver"]["expires"],
            &doc["waiver"]["added"],
            &doc["allowedDifference"]["rationale"],
        ] {
            assert_eq!(*sentinel, json!(UNREVIEWED));
        }
    }

    #[test]
    fn a_bare_channel_observable_path_is_refused() {
        // The FR-041 rule, driven directly. A bare channel tolerates everything on that
        // channel forever, which is a global ignore list wearing a waiver id.
        for blanket in [
            "chan-structured-output",
            "chan-structured-output.",
            ".configuration",
            "",
        ] {
            assert!(
                matches!(
                    reject_blanket_observable_path("fnd-x", blanket),
                    Err(PromotionError::UnscopedTolerance { .. })
                ),
                "{blanket:?} must be refused"
            );
        }
        reject_blanket_observable_path("fnd-x", "chan-structured-output.configuration")
            .expect("a scoped path is accepted");
    }

    #[test]
    fn a_signature_with_no_path_cannot_be_tolerated_at_all() {
        let f = finding(signature(CHAN_STRUCTURED_OUTPUT, ""));
        assert!(matches!(
            tolerance_skeleton(&f),
            Err(PromotionError::UnscopedTolerance { .. })
        ));
    }

    #[test]
    fn an_outcome_divergence_becomes_a_directional_strictness_expectation() {
        let mut f = finding(signature(CHAN_EXIT_CODE, "outcome"));
        f.witnesses[0].observed_values = ObservedValues {
            deacon: Some(json!("rejected")),
            reference: Some(json!("accepted")),
        };
        let doc = tolerance_skeleton(&f).expect("scaffolds");
        assert_eq!(doc["waiver"]["expect"]["kind"], json!("deacon-stricter"));

        f.witnesses[0].observed_values = ObservedValues {
            deacon: Some(json!("accepted")),
            reference: Some(json!("rejected")),
        };
        let doc = tolerance_skeleton(&f).expect("scaffolds");
        assert_eq!(doc["waiver"]["expect"]["kind"], json!("reference-stricter"));
    }

    #[test]
    fn a_value_divergence_becomes_a_field_divergence_expectation() {
        let f = finding(signature(
            CHAN_STRUCTURED_OUTPUT,
            "configuration.remoteUser",
        ));
        let doc = tolerance_skeleton(&f).expect("scaffolds");
        assert_eq!(doc["waiver"]["expect"]["kind"], json!("field-divergence"));
        assert_eq!(doc["waiver"]["expect"]["ours"], json!("vscode"));
        assert_eq!(doc["waiver"]["expect"]["reference"], json!("root"));
    }

    #[test]
    fn the_scaffolded_behavior_fails_the_promotion_pre_flight_on_every_axis() {
        // The scaffold must never be committable unedited: its own output is exactly what
        // FR-038 rejects, and it says so by axis rather than as one lump.
        let f = finding(signature(
            CHAN_STRUCTURED_OUTPUT,
            "configuration.remoteUser",
        ));
        let doc = promotion_skeleton(&f).expect("scaffolds");
        let errors = validate_promotion(&f, &doc["behavior"], None, &[]);

        for axis in BEHAVIOR_DISPOSITION_AXES {
            assert!(
                errors.iter().any(|e| matches!(
                    e,
                    PromotionError::MissingDisposition { axis: a, .. } if *a == axis
                )),
                "`{axis}` must be reported BY NAME: {errors:?}"
            );
        }
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, PromotionError::MissingBehaviorIdentity { .. })),
            "the sentinel id is not an identity: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, PromotionError::UnresolvedCase { .. })),
            "a promotion naming no case names nothing: {errors:?}"
        );
    }

    #[test]
    fn a_fully_decided_promotion_naming_a_declared_case_passes_the_pre_flight() {
        let mut f = finding(signature(
            CHAN_STRUCTURED_OUTPUT,
            "configuration.remoteUser",
        ));
        f.state = FindingState::Triaged;
        f.classification = Some(Classification::DeaconRegression);
        let behavior = json!({
            "id": "bhv-readconfig-remote-user",
            "spec": "conformant",
            "reference": "divergent",
            "decision": "follow-spec",
        });
        assert_eq!(
            validate_promotion(&f, &behavior, Some("case-real"), &["case-real"]),
            Vec::new()
        );
    }

    /// Cross-check against the model, so renaming an axis on [`crate::model::BehaviorUnit`]
    /// fails here rather than leaving the pre-flight checking a field that no longer exists.
    #[test]
    fn the_axis_names_match_the_behavior_record() {
        use crate::model::{BehaviorUnit, Decision, ReferenceStatus, SpecStatus};
        let unit = BehaviorUnit {
            id: "bhv-x".to_string(),
            area: "read-configuration".to_string(),
            statement: "s".to_string(),
            applicability: Vec::new(),
            spec: SpecStatus::Conformant,
            reference: ReferenceStatus::Aligned,
            decision: Decision::FollowSpec,
            notes: None,
            scenario_applicability: Default::default(),
        };
        let rendered = serde_json::to_value(&unit).expect("a behavior serializes");
        for axis in BEHAVIOR_DISPOSITION_AXES {
            assert!(
                rendered.get(axis).is_some(),
                "`{axis}` is not a field of the behavior record any more; the promotion \
                 pre-flight would be checking something that does not exist"
            );
        }
    }
}
