//! Differential comparison of deacon against the verified pinned oracle over one
//! candidate (025-exploratory-parity-discovery, T034/T035/T036/T122,
//! FR-013 – FR-017).
//!
//! ## Nothing here is a new mechanism
//!
//! - **Execution** is [`crate::exec::run_and_capture`] — the same bounded invocation with
//!   always-on raw capture every parity comparison already uses. A second execution path
//!   would be a second set of bounds, captures, and failure modes.
//! - **Oracle resolution and exact-version verification** are [`crate::oracle`] and
//!   [`crate::prereq`]. A missing or mismatched oracle fails loudly (FR-003).
//! - **Normalization** is [`crate::normalize`], applied through exactly the rule chain the
//!   declarative `chan-structured-output` channel applies (FR-015). A signature computed
//!   from independently re-diffed values would be a second opinion on what differs, able
//!   to disagree with the one the comparison used — the identical defect class the
//!   single-normalizer rule exists to prevent.
//! - **The signature** is derived from `normalize::diff`'s own [`ConfigDivergence`]
//!   output by `deacon_conformance::discovery::signature` (T035): a field-for-field move,
//!   never a recomputation.
//!
//! ## Outcomes and structured content, never message wording (FR-016, T122)
//!
//! The comparison relates two things and no others: whether each side **accepted or
//! rejected** the candidate, and — when both accepted — the **normalized structured
//! document**. Diagnostic prose is captured to disk for a reviewer and is never compared.
//!
//! Two rejections that differ only in wording therefore produce no finding, by
//! construction rather than by a filter that could be forgotten: there is no code path
//! that reads stderr into a comparison. The exit *class* is compared rather than the
//! numeric code for the same reason — deacon and the reference legitimately spell "I
//! refused this" with different non-zero codes, and treating that as a difference would
//! report the wording of a status rather than its meaning.
//!
//! ## Already-characterized differences never enter the queue as new (FR-017, T036)
//!
//! A difference the project has already recorded — through a case's scoped
//! `allowedDifferences`, or through a `wvr-` waiver — is reported as
//! [`Verdict::Characterized`] naming the record that covers it. It is still *observed* and
//! still counted; it simply is not news. Without this the queue would fill on every run
//! with the strictness family this repository has already characterized in full, and the
//! genuinely new finding would be the one nobody could see.
//!
//! Note the direction: discovery **reads** the tolerance records and never writes one.
//! FR-018 forbids a discovery program authoring an allowed difference, because that would
//! let a difference disappear by being observed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use deacon_conformance::discovery::queue::ObservedValues;
use deacon_conformance::discovery::signature::{Divergence, DivergenceKind, Signature};
use deacon_conformance::load::Registry;
use deacon_conformance::model::{CHAN_EXIT_CODE, CHAN_STRUCTURED_OUTPUT, Expect, Scope};
use serde_json::Value;

use crate::HarnessError;
use crate::exec::{Invocation, Side, run_and_capture};
use crate::normalize::{self, ConfigDivergence, DiffKind, DocumentBlock};
use crate::oracle::VerifiedOracle;

/// Whether an implementation accepted or rejected a candidate.
///
/// The *class*, not the numeric exit code: two implementations spell "I refused this" with
/// different non-zero codes, and comparing the numbers would report the wording of a
/// status rather than its meaning (FR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeClass {
    /// The CLI exited successfully.
    Accepted,
    /// The CLI exited non-zero, or was terminated.
    Rejected,
}

impl OutcomeClass {
    /// The stable wire spelling, used as the compared value on `chan-exit-code`.
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomeClass::Accepted => "accepted",
            OutcomeClass::Rejected => "rejected",
        }
    }

    fn of(invocation: &Invocation) -> OutcomeClass {
        if invocation.success {
            OutcomeClass::Accepted
        } else {
            OutcomeClass::Rejected
        }
    }
}

/// What the comparison decided about one observed difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Not covered by any recorded case, waiver, or tolerated difference — a queue
    /// candidate.
    New,
    /// Covered by the named record (a `wvr-`/`ext-`/`case-` id). Reported, counted, and
    /// **never admitted to the queue as new** (FR-017).
    Characterized(String),
}

/// One observed difference: its signature, the concrete values behind it, and the verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// The deduplication key.
    pub signature: Signature,
    /// The concrete values each side produced — **evidence, not identity** (research D3).
    pub observed: ObservedValues,
    /// Whether this is news.
    pub verdict: Verdict,
}

impl Observation {
    /// Whether this observation should be admitted to the findings queue.
    pub fn is_new(&self) -> bool {
        self.verdict == Verdict::New
    }
}

/// Where one side's raw evidence was preserved (FR-014).
///
/// Raw and normalized are kept **separate**: the raw bytes live on disk exactly as the CLI
/// produced them, and the normalized document lives in memory alongside. Conflating them
/// would make it impossible to tell a difference the implementations produced from one the
/// normalizer produced — which is precisely the `normalizer-defect` classification the
/// triage vocabulary reserves.
#[derive(Debug, Clone, PartialEq)]
pub struct SideEvidence {
    /// Accepted or rejected.
    pub outcome: OutcomeClass,
    /// The raw exit code, preserved but never compared.
    pub exit_code: Option<i32>,
    /// Absolute path to the verbatim stdout capture.
    pub stdout_path: PathBuf,
    /// Absolute path to the verbatim stderr capture.
    pub stderr_path: PathBuf,
    /// The normalized structured document, when the side produced one.
    pub normalized: Option<Value>,
}

/// The result of comparing one candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct DifferentialResult {
    /// The candidate that was compared.
    pub candidate_id: String,
    /// deacon's evidence.
    pub deacon: SideEvidence,
    /// The reference's evidence.
    pub reference: SideEvidence,
    /// **Neither** side reached configuration resolution — the SC-002 numerator.
    ///
    /// Defined as "no side produced a structured document", which is exactly the
    /// complement of FR-007's "reach configuration resolution or a later stage". It is
    /// deliberately *not* "deacon rejected": a rejection by one side while the other
    /// resolves is a real difference, and counting it as a trivial failure would hide the
    /// most interesting candidate the generator can produce.
    pub parse_stage_failure: bool,
    /// Every observed difference, in the diff's deterministic order.
    pub observations: Vec<Observation>,
}

impl DifferentialResult {
    /// The observations that are news.
    pub fn new_observations(&self) -> impl Iterator<Item = &Observation> {
        self.observations.iter().filter(|o| o.is_new())
    }

    /// How many observations were already characterized.
    pub fn characterized_count(&self) -> usize {
        self.observations.iter().filter(|o| !o.is_new()).count()
    }
}

/// Everything one comparison needs. Bundled so the call site reads as the fact it is —
/// "compare this candidate, in this workspace, with these two binaries, under this bound".
#[derive(Debug, Clone, Copy)]
pub struct DifferentialInput<'a> {
    /// The candidate's `cnd-` id, used as the raw-artifact case name.
    pub candidate_id: &'a str,
    /// The materialized workspace both sides are pointed at.
    pub workspace: &'a Path,
    /// The deacon binary under test.
    pub deacon: &'a Path,
    /// The **verified** pinned oracle. Taking the verified type rather than a path is what
    /// makes "never compare against an unverified reference" (FR-003) a type-level fact
    /// at this call site rather than a rule the caller has to remember.
    pub oracle: &'a VerifiedOracle,
    /// The per-candidate bound (60 s hermetic, 5 min container-backed).
    pub bound: Duration,
    /// Where raw artifacts are written.
    pub report_root: &'a Path,
    /// Whether the candidate was **deliberately** made invalid — a near-valid draw that
    /// violates a `required` key, or a mutated document.
    ///
    /// This is what narrows the strictness characterization from a channel-wide ignore
    /// list to a scoped one. deacon deliberately rejects malformed configuration the
    /// reference accepts, and that family is characterized in full — but only *for
    /// malformed input*. deacon refusing a candidate the grammar says is **valid** is a
    /// `deacon-regression`, and it is exactly the finding this whole feature exists to
    /// surface, so it must never be swallowed by the same waiver.
    pub deliberately_invalid: bool,
}

/// Compare one candidate across both implementations.
pub async fn compare(
    input: DifferentialInput<'_>,
    characterization: &Characterization,
) -> Result<DifferentialResult, HarnessError> {
    let workspace = input.workspace.to_string_lossy().into_owned();
    let args: Vec<&str> = vec!["read-configuration", "--workspace-folder", &workspace];

    let deacon_run = run_and_capture(
        Side::Deacon,
        "discovery_campaign",
        input.candidate_id,
        input.deacon,
        &args,
        input.workspace,
        input.bound,
        input.report_root,
    )
    .await?;
    let oracle_run = run_and_capture(
        Side::Oracle,
        "discovery_campaign",
        input.candidate_id,
        &input.oracle.path,
        &args,
        input.workspace,
        input.bound,
        input.report_root,
    )
    .await?;

    // Structured content is read only from a side that ACCEPTED. A rejecting CLI's stdout
    // is diagnostic prose or nothing, and treating it as a document would smuggle message
    // wording into the comparison through the back door (FR-016).
    let deacon_doc = structured_document(&deacon_run, input.workspace, Side::Deacon);
    let reference_doc = structured_document(&oracle_run, input.workspace, Side::Oracle);

    let deacon = SideEvidence {
        outcome: OutcomeClass::of(&deacon_run),
        exit_code: deacon_run.exit_code,
        stdout_path: deacon_run.stdout_path(),
        stderr_path: deacon_run.stderr_path(),
        normalized: deacon_doc.clone(),
    };
    let reference = SideEvidence {
        outcome: OutcomeClass::of(&oracle_run),
        exit_code: oracle_run.exit_code,
        stdout_path: oracle_run.stdout_path(),
        stderr_path: oracle_run.stderr_path(),
        normalized: reference_doc.clone(),
    };

    Ok(result_from_sides(
        input.candidate_id,
        deacon,
        reference,
        characterization,
        input.deliberately_invalid,
    ))
}

/// Relate two sides' evidence into a [`DifferentialResult`].
///
/// Extracted from [`compare`] rather than duplicated, and it is the **only** place a
/// comparison is defined. The pipeline proof (`super::pipeline_proof`) acquires its two
/// sides differently — one of them is perturbed at the sealed evidence-source boundary —
/// but it must relate them exactly as a live campaign does, or the proof would be asserting
/// that *a* comparison propagates a difference rather than that *this* one does.
pub(crate) fn result_from_sides(
    candidate_id: &str,
    deacon: SideEvidence,
    reference: SideEvidence,
    characterization: &Characterization,
    deliberately_invalid: bool,
) -> DifferentialResult {
    let observations =
        observations_between(&deacon, &reference, characterization, deliberately_invalid);
    DifferentialResult {
        candidate_id: candidate_id.to_string(),
        parse_stage_failure: deacon.normalized.is_none() && reference.normalized.is_none(),
        deacon,
        reference,
        observations,
    }
}

/// Every difference between two sides' evidence, in the diff's deterministic order.
///
/// Three branches, and the middle one is the reason this is not simply "diff the two
/// documents": a CLI that exits zero having emitted nothing parseable would otherwise be
/// reported as agreement, which is the quietest possible failure and exactly the kind this
/// feature exists to find.
fn observations_between(
    deacon: &SideEvidence,
    reference: &SideEvidence,
    characterization: &Characterization,
    deliberately_invalid: bool,
) -> Vec<Observation> {
    let mut observations = Vec::new();

    // `chan-exit-code`: the OUTCOME CLASS, never the numeric code and never the message.
    if deacon.outcome != reference.outcome {
        let deacon_value = Value::String(deacon.outcome.as_str().to_string());
        let reference_value = Value::String(reference.outcome.as_str().to_string());
        let signature = Signature::derive(
            CHAN_EXIT_CODE,
            &Divergence {
                kind: DivergenceKind::Value,
                path: "outcome",
                deacon: Some(&deacon_value),
                reference: Some(&reference_value),
            },
        );
        let stricter = if deacon.outcome == OutcomeClass::Rejected {
            Stricter::Deacon
        } else {
            Stricter::Reference
        };
        let verdict = characterization.verdict(
            &signature,
            ObservationContext {
                deliberately_invalid,
                stricter: Some(stricter),
            },
        );
        observations.push(Observation {
            signature,
            observed: ObservedValues {
                deacon: Some(deacon_value),
                reference: Some(reference_value),
            },
            verdict,
        });
    }

    // One side succeeded and produced a structured document while the other succeeded and
    // did not. Without this branch that difference is invisible: the outcome classes agree
    // (both accepted), and the document comparison below needs both documents, so a CLI
    // that exited zero having emitted nothing parseable would be reported as agreement.
    // That is the quietest possible failure and exactly the kind this feature exists to
    // find, so it is a difference in its own right.
    if deacon.outcome == reference.outcome
        && deacon.normalized.is_some() != reference.normalized.is_some()
    {
        let (kind, deacon_value, reference_value) = if deacon.normalized.is_some() {
            (
                DivergenceKind::DeaconOnly,
                Some(Value::String("structured document".to_string())),
                None,
            )
        } else {
            (
                DivergenceKind::RefOnly,
                None,
                Some(Value::String("structured document".to_string())),
            )
        };
        let signature = Signature::derive(
            CHAN_STRUCTURED_OUTPUT,
            &Divergence {
                kind,
                path: "document",
                deacon: deacon_value.as_ref(),
                reference: reference_value.as_ref(),
            },
        );
        let verdict = characterization.verdict(
            &signature,
            ObservationContext {
                deliberately_invalid,
                stricter: None,
            },
        );
        observations.push(Observation {
            signature,
            observed: ObservedValues {
                deacon: deacon_value,
                reference: reference_value,
            },
            verdict,
        });
    }

    // `chan-structured-output`: the normalized documents, compared only when BOTH sides
    // produced one. Comparing a document against an absence would re-report the presence
    // difference already recorded above, once per key.
    if let (Some(d), Some(r)) = (&deacon.normalized, &reference.normalized) {
        for divergence in normalize::diff(d, r) {
            let signature = signature_of(CHAN_STRUCTURED_OUTPUT, &divergence);
            let verdict = characterization.verdict(
                &signature,
                ObservationContext {
                    deliberately_invalid,
                    // Not an outcome divergence: both sides resolved, so neither was
                    // stricter than the other about accepting the input.
                    stricter: None,
                },
            );
            observations.push(Observation {
                signature,
                observed: ObservedValues {
                    deacon: divergence.deacon.clone(),
                    reference: divergence.reference.clone(),
                },
                verdict,
            });
        }
    }

    observations
}

/// **T035** — derive a signature from `normalize::diff`'s own output.
///
/// A field-for-field move, with no recomputation of what differs. `ConfigDivergence` and
/// `Divergence` carry the same four fields, and this is the single place the two
/// vocabularies meet — which is what keeps "derive only, never re-diff" true in the code
/// rather than only in a comment (research D3).
pub fn signature_of(channel: &str, divergence: &ConfigDivergence) -> Signature {
    let kind = match divergence.kind {
        DiffKind::RefOnly => DivergenceKind::RefOnly,
        DiffKind::DeaconOnly => DivergenceKind::DeaconOnly,
        DiffKind::Value => DivergenceKind::Value,
    };
    Signature::derive(
        channel,
        &Divergence {
            kind,
            path: &divergence.path,
            deacon: divergence.deacon.as_ref(),
            reference: divergence.reference.as_ref(),
        },
    )
}

/// The normalized structured document a side produced, or `None` when it produced none.
///
/// Applies exactly the `chan-structured-output` rule chain from [`crate::normalize`] —
/// `path_token` then `config_document_rules`, in that order, because that is the chain
/// `normalize`'s own per-channel dispatch applies for this channel. Restating the rules
/// here would be the second normalization path FR-015 forbids; *calling* them in the
/// declared order is the reuse it requires.
fn structured_document(invocation: &Invocation, workspace: &Path, side: Side) -> Option<Value> {
    structured_document_bytes(invocation.success, &invocation.stdout, workspace, side)
}

/// The same normalization, over raw captured bytes rather than an [`Invocation`].
///
/// The pipeline proof needs this seam: its deacon side is the stdout of a real run **after**
/// a perturbation has been applied to it at the sealed evidence-source boundary, so there is
/// no `Invocation` carrying those bytes — and reconstructing one would mean inventing a
/// capture that never happened. Taking the bytes keeps the proof on the single normalization
/// chain FR-015 permits instead of growing a second one beside it.
pub(crate) fn structured_document_bytes(
    success: bool,
    stdout: &[u8],
    workspace: &Path,
    side: Side,
) -> Option<Value> {
    if !success {
        return None;
    }
    let text = String::from_utf8_lossy(stdout);
    let raw: Value = serde_json::from_str(text.trim()).ok()?;
    let tokens = normalize::tokens_for_channel(CHAN_STRUCTURED_OUTPUT, workspace);
    Some(normalize::config_document_rules(
        &normalize::path_token(&raw, &tokens),
        side,
        DocumentBlock::Wrapper,
    ))
}

// ---------------------------------------------------------------------------
// T036 — already-characterized suppression
// ---------------------------------------------------------------------------

/// The project's recorded tolerances, indexed for lookup by signature (FR-017).
///
/// Built from records the registry already owns. Discovery **reads** them and never writes
/// one: FR-018 forbids a discovery program authoring an allowed difference, because a
/// difference that could be tolerated by being observed would disappear precisely when it
/// was found.
#[derive(Debug, Clone, Default)]
pub struct Characterization {
    /// `(channel, path prefix, covering id)` from every case's scoped
    /// `allowedDifferences`. The `observablePath` is a dotted path **within** a channel,
    /// never a bare channel — the registry rejects a bare-channel scope at load (V19), so
    /// there is no global ignore list to inherit here.
    scoped_paths: Vec<(String, String, String)>,
    /// `(field pattern, waiver id)` from every `StateField`-scoped waiver. The pattern
    /// supports an exact match or a trailing `*`, matched by the SAME
    /// [`crate::waiver::field_matches`] the parity comparison uses.
    field_patterns: Vec<(String, String)>,
    /// `(which side is stricter, waiver id)` for every waiver characterizing an
    /// accept/reject **strictness** divergence.
    ///
    /// deacon's deliberate strictness on malformed configuration is characterized in full
    /// (`bhv-readconfig-*-rejected` and their waivers), and a near-valid generator
    /// reproduces it constantly — so without this the queue would fill on every run with
    /// the one divergence family the project has most thoroughly reviewed.
    ///
    /// It is deliberately **not** a channel-wide tolerance, which would be the global
    /// ignore list the registry itself forbids (V19). Two conditions narrow it, and both
    /// are load-bearing:
    ///
    /// 1. **The direction must match.** A `deacon-stricter` waiver says deacon refuses
    ///    what the reference accepts. It says nothing about deacon *accepting* what the
    ///    reference refuses, which is the opposite defect.
    /// 2. **The candidate must be deliberately invalid.** These waivers characterize
    ///    strictness *on malformed input*. deacon refusing a candidate the grammar says is
    ///    valid is a `deacon-regression` — precisely the finding this feature exists to
    ///    surface — and swallowing it under a malformed-input waiver would make the
    ///    machinery quietest exactly where it should be loudest.
    strictness_waivers: Vec<(Stricter, String)>,
}

/// Which implementation refused an input the other accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stricter {
    /// deacon rejected; the reference accepted.
    Deacon,
    /// The reference rejected; deacon accepted.
    Reference,
}

/// What the comparison knows about a candidate when it asks whether a difference is
/// already characterized.
///
/// A separate parameter rather than fields on [`Signature`] on purpose: a signature is the
/// deduplication *identity*, and folding "was this input deliberately malformed?" into it
/// would split one defect across two signatures depending on how the candidate that
/// surfaced it happened to be produced.
#[derive(Debug, Clone, Copy, Default)]
pub struct ObservationContext {
    /// Whether the candidate was deliberately made invalid (a near-valid draw or a mutated
    /// document).
    pub deliberately_invalid: bool,
    /// For an outcome divergence, which side refused. `None` for every other channel.
    pub stricter: Option<Stricter>,
}

impl Characterization {
    /// Build the index from a loaded registry.
    pub fn from_registry(registry: &Registry) -> Characterization {
        let mut scoped_paths = Vec::new();
        for case in &registry.cases {
            for allowed in &case.allowed_differences {
                let Some((channel, path)) = allowed.observable_path.split_once('.') else {
                    // A bare channel is a global ignore list, which the registry rejects at
                    // load (V19). Skipping it here rather than treating it as a
                    // channel-wide tolerance means a record that somehow got through
                    // suppresses nothing, instead of silently suppressing everything.
                    continue;
                };
                let covering = allowed
                    .waiver_id
                    .clone()
                    .or_else(|| allowed.divergence_id.clone())
                    .unwrap_or_else(|| case.id.clone());
                scoped_paths.push((channel.to_string(), path.to_string(), covering));
            }
        }

        let mut field_patterns = Vec::new();
        let mut strictness_waivers = Vec::new();
        for waiver in &registry.waivers {
            match &waiver.scope {
                Scope::StateField { field, .. } => {
                    field_patterns.push((field.clone(), waiver.id.clone()));
                }
                Scope::CorpusCase { .. } => {}
            }
            match waiver.expect {
                Expect::DeaconStricter { .. } => {
                    strictness_waivers.push((Stricter::Deacon, waiver.id.clone()));
                }
                Expect::ReferenceStricter { .. } => {
                    strictness_waivers.push((Stricter::Reference, waiver.id.clone()));
                }
                _ => {}
            }
        }
        strictness_waivers.sort_by(|a, b| a.1.cmp(&b.1));
        strictness_waivers.dedup();

        Characterization {
            scoped_paths,
            field_patterns,
            strictness_waivers,
        }
    }

    /// The verdict for a signature: [`Verdict::Characterized`] naming the covering record,
    /// or [`Verdict::New`].
    pub fn verdict(&self, signature: &Signature, context: ObservationContext) -> Verdict {
        match self.covering(signature, context) {
            Some(id) => Verdict::Characterized(id),
            None => Verdict::New,
        }
    }

    /// The id of the record covering `signature`, if any.
    pub fn covering(&self, signature: &Signature, context: ObservationContext) -> Option<String> {
        // A scoped allowed difference covers its exact path and everything beneath it —
        // `chan-structured-output.configuration` covers `…configuration.remoteUser`. It
        // does NOT cover a sibling whose name merely shares a prefix, which is why the
        // match requires a `.` boundary rather than a bare `starts_with`.
        for (channel, path, covering) in &self.scoped_paths {
            if channel != &signature.channel {
                continue;
            }
            if &signature.path == path || signature.path.starts_with(&format!("{path}.")) {
                return Some(covering.clone());
            }
        }

        for (pattern, waiver) in &self.field_patterns {
            if crate::waiver::field_matches(&signature.path, pattern) {
                return Some(waiver.clone());
            }
        }

        // A strictness waiver characterizes deacon-vs-reference acceptance ON MALFORMED
        // INPUT, in one direction. Both conditions must hold, or the tolerance would be a
        // channel-wide ignore list wearing a waiver id.
        if signature.channel == CHAN_EXIT_CODE
            && context.deliberately_invalid
            && let Some(stricter) = context.stricter
        {
            return self
                .strictness_waivers
                .iter()
                .find(|(side, _)| *side == stricter)
                .map(|(_, id)| id.clone());
        }

        None
    }

    /// Whether anything at all is indexed — a campaign logs this, because an empty index
    /// against a populated registry means every known divergence would be re-reported as
    /// news.
    pub fn is_empty(&self) -> bool {
        self.scoped_paths.is_empty()
            && self.field_patterns.is_empty()
            && self.strictness_waivers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn signature(channel: &str, path: &str) -> Signature {
        let d = json!("a");
        let r = json!("b");
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

    #[test]
    fn the_signature_is_derived_from_the_diffs_own_output() {
        // T035: a field-for-field move. Re-deriving `kind`, `path`, or the value shape here
        // would be a second opinion on what differs, able to disagree with the one the
        // comparison used.
        for (kind, expected) in [
            (DiffKind::RefOnly, DivergenceKind::RefOnly),
            (DiffKind::DeaconOnly, DivergenceKind::DeaconOnly),
            (DiffKind::Value, DivergenceKind::Value),
        ] {
            let divergence = ConfigDivergence {
                kind,
                path: "configuration.remoteUser".to_string(),
                deacon: Some(json!("vscode")),
                reference: Some(json!("root")),
            };
            let sig = signature_of(CHAN_STRUCTURED_OUTPUT, &divergence);
            assert_eq!(sig.kind, expected);
            assert_eq!(sig.path, "configuration.remoteUser");
            assert_eq!(sig.channel, CHAN_STRUCTURED_OUTPUT);
            assert_eq!(sig.derived_id(), sig.id);
        }
    }

    /// The context of an ordinary structured-output observation.
    fn plain() -> ObservationContext {
        ObservationContext::default()
    }

    /// The context of an outcome divergence on a deliberately malformed candidate.
    fn refused(stricter: Stricter) -> ObservationContext {
        ObservationContext {
            deliberately_invalid: true,
            stricter: Some(stricter),
        }
    }

    #[test]
    fn an_empty_characterization_calls_everything_new() {
        let c = Characterization::default();
        assert!(c.is_empty());
        assert_eq!(
            c.verdict(
                &signature(CHAN_STRUCTURED_OUTPUT, "configuration.remoteUser"),
                plain()
            ),
            Verdict::New
        );
        assert_eq!(
            c.verdict(
                &signature(CHAN_EXIT_CODE, "outcome"),
                refused(Stricter::Deacon)
            ),
            Verdict::New
        );
    }

    #[test]
    fn a_scoped_allowed_difference_covers_its_subtree_but_not_a_prefix_sibling() {
        let c = Characterization {
            scoped_paths: vec![(
                CHAN_STRUCTURED_OUTPUT.to_string(),
                "configuration.remoteEnv".to_string(),
                "wvr-example".to_string(),
            )],
            ..Characterization::default()
        };
        assert_eq!(
            c.verdict(
                &signature(CHAN_STRUCTURED_OUTPUT, "configuration.remoteEnv"),
                plain()
            ),
            Verdict::Characterized("wvr-example".to_string())
        );
        assert_eq!(
            c.verdict(
                &signature(CHAN_STRUCTURED_OUTPUT, "configuration.remoteEnv.PATH"),
                plain()
            ),
            Verdict::Characterized("wvr-example".to_string())
        );
        assert_eq!(
            c.verdict(
                &signature(CHAN_STRUCTURED_OUTPUT, "configuration.remoteEnvironment"),
                plain()
            ),
            Verdict::New,
            "a sibling that merely shares a textual prefix is a different path, and \
             tolerating it would silence a difference nobody reviewed"
        );
        assert_eq!(
            c.verdict(
                &signature(CHAN_EXIT_CODE, "configuration.remoteEnv"),
                plain()
            ),
            Verdict::New,
            "a tolerance is scoped to its channel"
        );
    }

    #[test]
    fn a_state_field_waiver_pattern_covers_its_field() {
        let c = Characterization {
            field_patterns: vec![("mounts.*".to_string(), "wvr-mounts".to_string())],
            ..Characterization::default()
        };
        assert_eq!(
            c.verdict(
                &signature(CHAN_STRUCTURED_OUTPUT, "mounts.0.source"),
                plain()
            ),
            Verdict::Characterized("wvr-mounts".to_string())
        );
        assert_eq!(
            c.verdict(&signature(CHAN_STRUCTURED_OUTPUT, "remoteUser"), plain()),
            Verdict::New
        );
    }

    #[test]
    fn a_strictness_waiver_covers_only_its_direction_on_deliberately_invalid_input() {
        let c = Characterization {
            strictness_waivers: vec![(Stricter::Deacon, "wvr-readconfig-strict".to_string())],
            ..Characterization::default()
        };
        let outcome = signature(CHAN_EXIT_CODE, "outcome");

        assert_eq!(
            c.verdict(&outcome, refused(Stricter::Deacon)),
            Verdict::Characterized("wvr-readconfig-strict".to_string()),
            "deacon's deliberate strictness on malformed input is characterized in full; a \
             near-valid generator reproduces it constantly and it is not news"
        );

        // The opposite direction is the opposite defect, and this waiver says nothing
        // about it.
        assert_eq!(
            c.verdict(&outcome, refused(Stricter::Reference)),
            Verdict::New,
            "a `deacon-stricter` waiver does not characterize deacon ACCEPTING what the \
             reference refuses"
        );

        // The condition that matters most: deacon refusing a candidate the grammar says is
        // VALID is a regression, and must not be swallowed by a malformed-input waiver.
        assert_eq!(
            c.verdict(
                &outcome,
                ObservationContext {
                    deliberately_invalid: false,
                    stricter: Some(Stricter::Deacon),
                }
            ),
            Verdict::New,
            "deacon rejecting a VALID candidate is the finding this whole feature exists to \
             surface — silencing it under a malformed-input waiver would make the machinery \
             quietest exactly where it should be loudest"
        );

        assert_eq!(
            c.verdict(
                &signature(CHAN_STRUCTURED_OUTPUT, "configuration.remoteUser"),
                refused(Stricter::Deacon)
            ),
            Verdict::New,
            "a strictness waiver says nothing about a value difference between two \
             successful resolutions"
        );
    }

    #[test]
    fn the_workspace_registry_indexes_real_tolerances() {
        // A guard against the index silently going empty: an empty index against a
        // populated registry would re-report every already-characterized divergence as
        // news, which is the flood FR-017 exists to prevent.
        let registry = Registry::load(&crate::conformance_registry_root())
            .expect("the committed registry loads");
        let c = Characterization::from_registry(&registry);
        assert!(
            !c.is_empty(),
            "the committed registry records tolerances; an empty index means the reader \
             stopped seeing them"
        );
        assert!(
            !c.strictness_waivers.is_empty(),
            "the read-configuration strictness family is characterized in the registry"
        );
    }

    #[test]
    fn the_outcome_class_is_the_comparison_not_the_exit_code() {
        // FR-016 / T122 in miniature: the values compared on `chan-exit-code` are the two
        // class names, so two rejections are equal whatever their numeric codes or
        // messages.
        assert_eq!(OutcomeClass::Accepted.as_str(), "accepted");
        assert_eq!(OutcomeClass::Rejected.as_str(), "rejected");
        assert_eq!(OutcomeClass::Rejected, OutcomeClass::Rejected);
        assert_ne!(OutcomeClass::Accepted, OutcomeClass::Rejected);
    }
}
