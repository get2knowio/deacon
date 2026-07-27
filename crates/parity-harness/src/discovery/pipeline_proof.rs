//! The injected-difference pipeline proof
//! (025-exploratory-parity-discovery, US5, T081/T082, FR-042a, research D7).
//!
//! Injects a known difference through [`crate::inject::perturb_source`] — the existing
//! **sealed** `EvidenceSource` boundary — and requires it to traverse
//! **generation → comparison → minimization → candidate → classification → promotable**.
//!
//! # Why the sealed boundary, and what reusing it buys
//!
//! `inject.rs` already establishes the one property this proof cannot cheaply
//! re-establish: every perturbation entry point is generic over a **sealed**
//! [`EvidenceSource`](crate::inject::EvidenceSource) trait that only
//! [`RunContext`] — the RAW captured artifact — implements. No
//! observer's return type can implement it, and its supertrait lives in a private module,
//! so injecting *downstream* of the comparison is not a rule to remember: it does not
//! compile.
//!
//! That matters here more than it does for `coverage-regressions`. A proof that could plant
//! a difference past the comparison would demonstrate nothing at all — it would be asserting
//! on data it wrote itself, downstream of the part under test. Reusing the boundary inherits
//! the guarantee rather than re-arguing it.
//!
//! It inherits [`HarnessError::InjectionInapplicable`] too, and that is the other half of
//! FR-042a: **a perturbation that never landed is reported as a harness fault, never as "the
//! pipeline found nothing"**. Those are opposite conclusions — one says the machinery is
//! mis-authored, the other says the implementations agree — and a proof that merged them
//! would be the most comfortable possible way for this feature to be broken.
//!
//! # What it compares against, and why it is not the oracle
//!
//! The counterpart is **deacon's own unperturbed run**, not the pinned reference.
//!
//! This is deliberate and it is what makes the proof mean anything. A reference comparison
//! has a baseline nobody controls: deacon and the oracle may already differ on a generated
//! candidate, and an injected difference landing on top of that cannot be attributed to the
//! injection. Comparing a run against itself makes the baseline empty — and [`establish`]
//! fails loudly rather than assuming it, so **every observation that appears afterwards is
//! the one that was planted**. That is the whole attribution argument, and it is the same
//! clean-baseline requirement [`crate::inject::detects`] imposes on a channel verdict.
//!
//! It also means the proof needs **no oracle, no Docker, and no network**: it asserts a
//! property of the *machinery*, and the reference takes no part in whether a difference
//! propagates from evidence to a promotable finding. That is the same argument research D12
//! makes for the metamorphic tier having no prerequisite, applied to the one command whose
//! exit status is supposed to mean "the pipeline is broken" rather than "a prerequisite was
//! missing".
//!
//! Two consequences follow, both load-bearing:
//!
//! - **Both sides are normalized as [`Side::Deacon`]**. The single normalizer is
//!   deliberately side-asymmetric (`drop_absent_optional` runs on deacon's `configuration`
//!   block only, 024 T123), so labelling one copy of deacon's output as the reference would
//!   apply rules written for a different serializer and manufacture differences the pipeline
//!   never found. Note what the empty-baseline check does and does not establish: because
//!   both sides are normalized identically it is *satisfied by construction*, so it
//!   confirms attribution — the two sides as this module builds them diff to nothing — but
//!   it does **not** independently vindicate the side choice. The argument for that is the
//!   one above, and a refactor that made the two sides asymmetric would be caught by the
//!   check rather than by the reasoning.
//! - **The tolerance index is not consulted.** [`Characterization`] answers "has this
//!   deacon-vs-reference difference already been characterized?", a question with no meaning
//!   for a self-comparison — and a scoped waiver that happened to cover the injected path
//!   would suppress the planted difference and report a working pipeline as broken. Left out
//!   visibly, for the same reason `run_metamorphic` leaves it out.
//!
//! # What it must never do
//!
//! Write anything the registry owns (FR-036). It emits reviewable candidates under
//! `<report_root>/proof/` and a report at `target/discovery/proof.json`, and it reaches the
//! promotion stage by *refusing* to promote: the stage passes when the state machine permits
//! promotion **and** [`promote::validate_promotion`] still rejects the scaffolded record for
//! every missing axis. Review-only promotion is proven by showing the gate holds, not by
//! going through it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use deacon_conformance::discovery::generate::{
    Candidate, Generator, required_keys, unsafe_reasons,
};
use deacon_conformance::discovery::grammar::Grammar;
use deacon_conformance::discovery::promote::{self, PromotionError};
use deacon_conformance::discovery::queue::{
    Campaign, CampaignLane, CampaignTier, Classification, Finding, FindingState, ObservedValues,
    PinnedInputSet, Witness,
};
use deacon_conformance::discovery::shrink::{
    self, ReductionInput, Reproduction, ReproductionProbe,
};
use deacon_conformance::discovery::signature::Signature;
use deacon_conformance::load::Registry;
use deacon_conformance::regression::{RegressionFile, RegressionRecord};
use serde_json::Value;

use crate::HarnessError;
use crate::exec::{Side, run_and_capture};
use crate::inject::{RegressionHarness, perturb_source};
use crate::observe::{ProcessOutcome, RunContext};

use super::campaign::{CandidateWorkspace, materialize_document, pinned_input_set};
use super::candidate::{self, CANDIDATE_PARTS, CandidateInputs, ReferenceProvenance};
use super::differential::{
    self, Characterization, DifferentialResult, Observation, OutcomeClass, SideEvidence,
};

/// The artifact-tree binary name every proof invocation captures under.
const PROOF_BINARY: &str = "discovery_proof";

/// The operation id the perturbation is applied to. One operation per run
/// (`read-configuration`), so the name is fixed.
const PROOF_OPERATION: &str = "read-configuration";

/// The injections the proof requires to traverse, as strict-JSON [`RegressionRecord`]s.
///
/// **Exactly the two channels this comparison reads.** The configuration differential
/// relates two things and no others (FR-016): whether each side *accepted or rejected* the
/// candidate, and — when both accepted — the *normalized structured document*. A record
/// perturbing stderr would be correctly invisible, because no code path reads stderr into a
/// comparison, and requiring it to surface would demand a defect rather than a proof.
///
/// These are **not** registry records. They are never loaded by `validate`, carry no `reg-`
/// obligation, and `expectedDetectingCases` — required non-empty by the shape — plays no
/// part here: the proof reports which *stages* a difference reached, not which cases caught
/// it.
const PROOF_INJECTIONS_JSON: &str = r#"{
  "records": [
    {
      "id": "reg-proof-structured-output",
      "channel": "chan-structured-output",
      "target": "structured-output-document",
      "perturbation": {
        "kind": "set-json-pointer",
        "pointer": "/configuration/deaconPipelineProofMarker",
        "value": "injected-by-the-pipeline-proof"
      },
      "expectedDetectingCases": ["discovery-proof"],
      "notes": "Writes through /configuration, which every accepted read-configuration document carries, so the perturbation lands on any candidate the proof can establish a clean baseline from. The key is not in ABSENT_OPTIONAL_KEYS and its value is non-empty, so no normalization rule can elide it — the difference the pipeline must surface is the one that was planted."
    },
    {
      "id": "reg-proof-exit-code",
      "channel": "chan-exit-code",
      "target": "process-result",
      "perturbation": { "kind": "set-exit-code", "exitCode": 9 },
      "expectedDetectingCases": ["discovery-proof"],
      "notes": "Flips the perturbed side to REJECTED while the unperturbed counterpart accepted, which is an outcome-class divergence. The comparison compares the class, never the numeric code, so 9 is arbitrary and the assertion is about meaning rather than spelling."
    }
  ]
}"#;

/// The proof's declared injections, parsed from `PROOF_INJECTIONS_JSON`.
///
/// Parsed rather than constructed so the records go through the SAME strict loader a
/// committed `reg-` record does: a proof whose perturbations were built by hand could
/// declare a shape the real harness would reject.
pub fn proof_injections() -> Result<Vec<RegressionRecord>, HarnessError> {
    let file: RegressionFile =
        serde_json::from_str(PROOF_INJECTIONS_JSON).map_err(|e| HarnessError::Report {
            cause: format!("the proof's own injection records do not load: {e}"),
        })?;
    if file.records.is_empty() {
        return Err(HarnessError::Report {
            cause: "the proof declares no injections; a proof over zero injections reports \
                    the same thing as one in which every injection traversed"
                .to_string(),
        });
    }
    Ok(file.records)
}

// ---------------------------------------------------------------------------
// Stages and verdicts
// ---------------------------------------------------------------------------

/// The pipeline stages FR-042a requires an injected difference to traverse, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// A candidate was drawn from the real constrained generator.
    Generation,
    /// The difference surfaced as an observation on the record's declared channel, against
    /// a provably clean baseline.
    Comparison,
    /// The real structural shrinker reduced the input while the signature held.
    Minimization,
    /// All six parts of a reviewable candidate were emitted.
    Candidate,
    /// The finding took exactly one classification through the real state machine.
    Classification,
    /// Promotion is permitted by the state machine **and** still gated on a human.
    Promotable,
}

impl Stage {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Generation => "generation",
            Stage::Comparison => "comparison",
            Stage::Minimization => "minimization",
            Stage::Candidate => "candidate",
            Stage::Classification => "classification",
            Stage::Promotable => "promotable",
        }
    }

    /// Every stage, in traversal order.
    pub fn all() -> &'static [Stage] {
        &[
            Stage::Generation,
            Stage::Comparison,
            Stage::Minimization,
            Stage::Candidate,
            Stage::Classification,
            Stage::Promotable,
        ]
    }
}

/// One stage's outcome, with the evidence behind it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageOutcome {
    pub stage: Stage,
    /// What was actually observed at this stage — recorded whether it passed or failed, so
    /// a failure names the thing that was wrong rather than only the stage it was in.
    pub detail: String,
}

/// What one injection's traversal concluded.
///
/// The three variants are three genuinely different facts, and keeping them apart is the
/// point of FR-042a: the pipeline worked; the pipeline swallowed a difference that was
/// there; or the proof itself was mis-authored and never planted anything.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum TraversalVerdict {
    /// Every stage was reached.
    Traversed,
    /// The perturbation LANDED and the difference did not reach the end of the pipeline.
    /// **A pipeline defect.**
    FailedToSurface { stage: Stage, cause: String },
    /// The perturbation never landed, so nothing at all was proven about the pipeline.
    /// **A proof defect** — deliberately not `failed-to-surface`, and deliberately not a
    /// pass: a mis-authored record must never masquerade as a working pipeline.
    InjectionInapplicable { cause: String },
}

/// One injection's traversal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Traversal {
    /// The perturbation record's id.
    pub injection: String,
    /// The channel it was declared on — the channel the difference had to surface on.
    pub channel: String,
    /// How many raw artifacts the perturbation was applied to (zero is impossible: it is
    /// [`TraversalVerdict::InjectionInapplicable`] before it gets here).
    pub applied: usize,
    /// The signature the difference produced, once the comparison surfaced it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// The finding id the signature derives — what a campaign would have admitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding: Option<String>,
    /// The stages reached, in order. Truncated at the first failure.
    pub stages: Vec<StageOutcome>,
    #[serde(flatten)]
    pub verdict: TraversalVerdict,
}

impl Traversal {
    /// Whether this traversal counts as a pass.
    pub fn traversed(&self) -> bool {
        self.verdict == TraversalVerdict::Traversed
    }
}

/// The `target/discovery/proof.json` document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofReport {
    pub schema_version: u32,
    /// The candidate the proof established its clean baseline from — the `cnd-` id, so a
    /// failure can be reproduced from the same seed.
    pub baseline_candidate: String,
    /// The seed, recorded verbatim, for the same reason a campaign records one.
    pub seed: String,
    /// One entry per declared injection, in declaration order.
    pub injections: Vec<Traversal>,
    /// Injections that landed and did not traverse — pipeline defects.
    pub failed_count: usize,
    /// Injections that never landed — proof defects.
    pub inapplicable_count: usize,
}

impl ProofReport {
    /// Roll traversals up into the report.
    pub fn build(seed: &str, baseline_candidate: &str, injections: Vec<Traversal>) -> ProofReport {
        let failed_count = injections
            .iter()
            .filter(|t| matches!(t.verdict, TraversalVerdict::FailedToSurface { .. }))
            .count();
        let inapplicable_count = injections
            .iter()
            .filter(|t| matches!(t.verdict, TraversalVerdict::InjectionInapplicable { .. }))
            .count();
        ProofReport {
            schema_version: 1,
            baseline_candidate: baseline_candidate.to_string(),
            seed: seed.to_string(),
            injections,
            failed_count,
            inapplicable_count,
        }
    }

    /// The run's exit status per contracts/discovery-cli.md: `0` when every injection
    /// traversed, `1` when any failed to surface **or** any was inapplicable.
    ///
    /// An **empty** injection set is also `1`. This is the one discovery command whose
    /// status depends on an outcome, and a proof over nothing reports exactly what a proof
    /// over everything-passing reports — the indistinguishability this whole feature exists
    /// to refuse.
    ///
    /// A function rather than a rule the bin restates, so the test asserting the status and
    /// the bin producing it are asserting the same decision.
    pub fn exit_status(&self) -> u8 {
        if self.injections.is_empty() {
            return 1;
        }
        if self.failed_count == 0 && self.inapplicable_count == 0 {
            0
        } else {
            1
        }
    }

    /// Byte-stable pretty JSON with a trailing newline.
    pub fn render(&self) -> Result<String, HarnessError> {
        let mut s = serde_json::to_string_pretty(self).map_err(|e| HarnessError::Report {
            cause: format!("could not serialize the proof report: {e}"),
        })?;
        s.push('\n');
        Ok(s)
    }
}

// ---------------------------------------------------------------------------
// Request and context
// ---------------------------------------------------------------------------

/// Everything one proof run needs, resolved by the caller (the bin, or a test).
///
/// Every knob is explicit rather than read from the environment, matching
/// [`super::campaign::CampaignRequest`]: mutating process-global state is `unsafe` under
/// this workspace's edition and hostile to a parallel runner besides.
#[derive(Debug, Clone)]
pub struct ProofRequest {
    /// The deacon binary under test — the ONLY implementation this proof runs.
    pub deacon_binary: PathBuf,
    /// The registry root, for the pins and the candidate's suggested behavior mapping.
    pub registry_dir: PathBuf,
    /// Where raw artifacts, proof candidates, and the report are written.
    pub report_root: PathBuf,
    /// The seed, exactly as recorded. A proof is as reproducible as a campaign.
    pub seed_hex: String,
    /// The seed's numeric value, feeding the generator's stream.
    pub seed: u64,
    /// The certification profile the run records itself under (a `prof-` id).
    pub profile: String,
    /// The per-invocation bound.
    pub bound: Duration,
    /// How many probes minimization may spend per injection.
    pub shrink_budget: u64,
    /// How many candidates may be drawn while looking for one deacon accepts.
    pub max_draws: u64,
}

/// The clean baseline every injection is traversed against.
///
/// Established once and shared: the baseline is a property of the *candidate*, not of the
/// injection, and re-establishing it per injection would spend a deacon invocation to reach
/// the same document.
pub struct ProofContext {
    registry: Registry,
    grammar: Grammar,
    pins: PinnedInputSet,
    campaign_id: String,
    /// The generated candidate that produced a clean baseline.
    candidate: Candidate,
    /// Its materialized workspace, reclaimed when the context drops.
    workspace: CandidateWorkspace,
    /// The unperturbed run's evidence — the counterpart every injected side is compared to.
    baseline: SideEvidence,
    /// The unperturbed run's raw process outcome, re-perturbed per injection.
    baseline_outcome: ProcessOutcome,
    request: ProofRequest,
}

impl ProofContext {
    /// The candidate the baseline was established from.
    pub fn candidate_id(&self) -> &str {
        &self.candidate.id
    }
}

/// Establish a clean baseline: draw candidates until one is safe **and** deacon accepts it
/// **and** comparing its evidence against itself yields nothing.
///
/// Every failure here is fail-loud. In particular, running out of draws is an error rather
/// than an empty report: a proof that could not find an input to plant a difference in has
/// proven nothing, and reporting that as "no injections failed" would be the exact
/// indistinguishability FR-042a forbids.
pub async fn establish(request: &ProofRequest) -> Result<ProofContext, HarnessError> {
    let registry = Registry::load(&request.registry_dir).map_err(|e| HarnessError::Report {
        cause: format!(
            "could not load the conformance registry at {}: {e}",
            request.registry_dir.display()
        ),
    })?;
    let grammar = Grammar::load_default().map_err(|e| HarnessError::Report {
        cause: format!("could not load the generation grammar: {e}"),
    })?;
    // No oracle is acquired — see the module docs. The pinned oracle *version* still enters
    // the pinned input set, because it is part of what makes any finding checkable, and the
    // pin is read from the committed `oracle.json` rather than from a binary.
    let pins = pinned_input_set(&grammar, None)?;
    let campaign_id = Campaign::derive_id(
        &request.seed_hex,
        &pins,
        CampaignLane::Invoked,
        &request.profile,
        // The closest declared tier: the proof draws generated configuration documents and
        // compares `read-configuration` evidence, exactly as the nightly tier does. Nothing
        // is persisted, so this id names a run rather than a record.
        CampaignTier::ConfigDifferential,
    );

    let mut generator = Generator::new(&grammar, request.seed);
    let mut rejected: Vec<String> = Vec::new();

    for _ in 0..request.max_draws {
        let candidate = generator.next_candidate();

        // FR-011: the same safety predicates a campaign applies. The proof runs real
        // deacon over a real generated document and must not be the one code path that
        // executes what a campaign would refuse.
        let refusals = unsafe_reasons(&candidate.document, /* container_backed */ false);
        if !refusals.is_empty() {
            rejected.push(format!(
                "{}: unsafe ({})",
                candidate.id,
                refusals.join("; ")
            ));
            continue;
        }

        let workspace =
            materialize_document(&candidate.document).map_err(|e| HarnessError::Report {
                cause: format!(
                    "could not materialize a proof workspace for `{}`: {e}",
                    candidate.id
                ),
            })?;

        let outcome = run_deacon(request, &candidate.id, workspace.path()).await?;
        let evidence = side_evidence(
            &outcome,
            workspace.path(),
            &request.report_root,
            &candidate.id,
        );

        // The baseline must be ACCEPTED and structured: a rejected run produces no document
        // for the structured-output injection to perturb, and no outcome-class difference
        // for the exit-code injection to create.
        if evidence.outcome != OutcomeClass::Accepted || evidence.normalized.is_none() {
            rejected.push(format!(
                "{}: deacon did not produce an accepted structured document",
                candidate.id
            ));
            continue;
        }

        // And it must be CLEAN against itself. This is what makes every later observation
        // attributable to the injection rather than to the candidate — the same
        // clean-baseline requirement `inject::detects` imposes, established here rather
        // than assumed.
        let self_comparison = differential::result_from_sides(
            &candidate.id,
            evidence.clone(),
            evidence.clone(),
            &Characterization::default(),
            false,
        );
        if !self_comparison.observations.is_empty() {
            return Err(HarnessError::Report {
                cause: format!(
                    "candidate `{}` differs from ITSELF in {} place(s) before anything was \
                     injected ({}). The comparison is not deterministic over one run's \
                     evidence, so no observation after an injection could be attributed to \
                     it — this is a comparison defect, not a proof that found nothing.",
                    candidate.id,
                    self_comparison.observations.len(),
                    self_comparison
                        .observations
                        .iter()
                        .map(|o| o.signature.path.clone())
                        .collect::<Vec<String>>()
                        .join(", ")
                ),
            });
        }

        return Ok(ProofContext {
            registry,
            grammar,
            pins,
            campaign_id,
            candidate,
            workspace,
            baseline: evidence,
            baseline_outcome: outcome,
            request: request.clone(),
        });
    }

    Err(HarnessError::Report {
        cause: format!(
            "no clean baseline in {} draw(s) from seed {}: {}. A proof with no input to \
             plant a difference in has proven nothing, so this is an error rather than a \
             report with no failures.",
            request.max_draws,
            request.seed_hex,
            rejected.join(" | ")
        ),
    })
}

/// Traverse one injected difference through the whole pipeline.
///
/// Public so a hermetic guard can drive an arbitrary record — including a deliberately
/// inapplicable one, which is how the "an injection that never lands fails LOUDLY rather
/// than reading as found-nothing" half of FR-042a is asserted rather than merely described.
pub async fn traverse(
    ctx: &ProofContext,
    record: &RegressionRecord,
) -> Result<Traversal, HarnessError> {
    let mut stages: Vec<StageOutcome> = Vec::new();
    let mut traversal = Traversal {
        injection: record.id.clone(),
        channel: record.channel.clone(),
        applied: 0,
        signature: None,
        finding: None,
        stages: Vec::new(),
        verdict: TraversalVerdict::Traversed,
    };

    // ---- generation -----------------------------------------------------
    stages.push(StageOutcome {
        stage: Stage::Generation,
        detail: format!(
            "candidate `{}` (draw {}, kind {:?}) drawn from seed {} by the real constrained \
             generator",
            ctx.candidate.id, ctx.candidate.index, ctx.candidate.kind, ctx.request.seed_hex
        ),
    });

    // ---- comparison -----------------------------------------------------
    let injected = match inject_and_compare(ctx, record, &ctx.candidate.document).await {
        Ok(injected) => injected,
        // The distinction FR-042a draws, and the reason this is not folded into the failure
        // path: the perturbation never landed, so NOTHING was proven about the pipeline. A
        // mis-authored record must not masquerade as a working one — and it must not
        // masquerade as a broken one either.
        Err(HarnessError::InjectionInapplicable { cause, .. }) => {
            traversal.stages = stages;
            traversal.verdict = TraversalVerdict::InjectionInapplicable { cause };
            return Ok(traversal);
        }
        Err(other) => return Err(other),
    };
    traversal.applied = injected.applied;

    let Some(observation) = injected.on_declared_channel(&record.channel) else {
        traversal.stages = stages;
        traversal.verdict = TraversalVerdict::FailedToSurface {
            stage: Stage::Comparison,
            cause: format!(
                "the perturbation was applied to {} raw artifact(s) and produced no NEW \
                 observation on `{}` (observed instead: {}). The baseline was clean, so a \
                 difference that is present in the evidence and absent from the comparison \
                 is the comparison losing it.",
                injected.applied,
                record.channel,
                describe(&injected.result)
            ),
        };
        return Ok(traversal);
    };
    let signature = observation.signature.clone();
    let finding_id = signature.finding_id();
    traversal.signature = Some(signature.id.clone());
    traversal.finding = Some(finding_id.clone());
    stages.push(StageOutcome {
        stage: Stage::Comparison,
        detail: format!(
            "surfaced as signature `{}` at `{}.{}` ({} / {}) against a provably empty \
             baseline",
            signature.id,
            signature.channel,
            signature.path,
            signature.kind.as_str(),
            signature.value_shape_class.as_str()
        ),
    });

    // ---- minimization ---------------------------------------------------
    let reduction_input = ReductionInput {
        document: ctx.candidate.document.clone(),
        mutations: ctx.candidate.mutations.clone(),
        required_keys: ctx
            .candidate
            .branch
            .map(|branch| required_keys(&ctx.grammar, branch))
            .unwrap_or_default(),
    };
    let mut probe = InjectedProbe {
        ctx,
        record,
        target: signature.clone(),
        probes: 0,
    };
    let reduction = shrink::reduce(&reduction_input, ctx.request.shrink_budget, &mut probe).await?;
    if reduction.probes == 0 {
        traversal.stages = stages;
        traversal.verdict = TraversalVerdict::FailedToSurface {
            stage: Stage::Minimization,
            cause: "the reduction spent zero probes, so nothing was reduced and nothing was \
                    tested; a reduction that never runs cannot establish that the signature \
                    is stable under it"
                .to_string(),
        };
        return Ok(traversal);
    }
    stages.push(StageOutcome {
        stage: Stage::Minimization,
        detail: format!(
            "reduced {} → {} node(s) in {} probe(s) via [{}]; isMinimal={}{}",
            reduction.original_size,
            reduction.reduced_size,
            reduction.probes,
            reduction.steps.join(", "),
            reduction.is_minimal,
            reduction
                .not_minimal_reason
                .as_ref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default(),
        ),
    });

    // ---- candidate ------------------------------------------------------
    let witness = Witness {
        id: Witness::derived_id(&ctx.campaign_id, &ctx.candidate.id),
        campaign_id: ctx.campaign_id.clone(),
        candidate_id: ctx.candidate.id.clone(),
        minimal_input: reduction.document.clone(),
        is_minimal: reduction.is_minimal,
        reduction_steps: reduction.steps.clone(),
        observed_values: ObservedValues {
            deacon: observation.observed.deacon.clone(),
            reference: observation.observed.reference.clone(),
        },
        mutation_operators: ctx.candidate.operator_ids(),
    };
    let candidates_root = ctx.request.report_root.join("proof").join("candidates");
    let dir = candidate::write(CandidateInputs {
        finding_id: &finding_id,
        signature: &signature,
        observation,
        campaign_id: &ctx.campaign_id,
        seed_hex: &ctx.request.seed_hex,
        lane: CampaignLane::Invoked.as_str(),
        tier: CampaignTier::ConfigDifferential.as_str(),
        profile: &ctx.request.profile,
        pinned_input_set: &ctx.pins,
        candidate_id: &ctx.candidate.id,
        operations: &ctx.candidate.operations,
        mutation_operators: &ctx.candidate.operator_ids(),
        reduction: &reduction,
        result: &injected.result,
        // NOT a reference comparison, and the provenance says so in its own file rather
        // than leaving a reviewer to infer it.
        reference: ReferenceProvenance::InjectedSelfComparison {
            injection: &record.id,
        },
        registry: &ctx.registry,
        root: &candidates_root,
    })?;
    if let Some(missing) = missing_parts(&dir) {
        traversal.stages = stages;
        traversal.verdict = TraversalVerdict::FailedToSurface {
            stage: Stage::Candidate,
            cause: format!(
                "the reviewable candidate at {} is missing {missing}; FR-027's claim is that \
                 reproducing needs only the candidate, and a missing part makes that false",
                dir.display()
            ),
        };
        return Ok(traversal);
    }
    stages.push(StageOutcome {
        stage: Stage::Candidate,
        detail: format!(
            "emitted all {} part(s) at {}",
            CANDIDATE_PARTS.len(),
            dir.display()
        ),
    });

    // ---- classification -------------------------------------------------
    let mut finding = Finding::newly_admitted(signature, witness, &ctx.campaign_id);
    // Any of the four promotable classifications would do; what is asserted is that the
    // REAL state machine accepts exactly one and then permits promotion, not which one a
    // reviewer would choose. A finding cannot tell you that, which is why promotion is a
    // human act in the first place.
    let classification = Classification::DeaconRegression;
    match finding.triage(
        classification,
        Some("injected by the FR-042a pipeline proof"),
    ) {
        Ok(FindingState::Triaged) => stages.push(StageOutcome {
            stage: Stage::Classification,
            detail: format!(
                "finding `{finding_id}` took classification `{}` and reached `triaged`",
                classification.as_str()
            ),
        }),
        Ok(other) => {
            traversal.stages = stages;
            traversal.verdict = TraversalVerdict::FailedToSurface {
                stage: Stage::Classification,
                cause: format!(
                    "triage left the finding in `{}` rather than `triaged`",
                    other.as_str()
                ),
            };
            return Ok(traversal);
        }
        Err(e) => {
            traversal.stages = stages;
            traversal.verdict = TraversalVerdict::FailedToSurface {
                stage: Stage::Classification,
                cause: format!("the state machine refused a classification: {e}"),
            };
            return Ok(traversal);
        }
    }

    // ---- promotable -----------------------------------------------------
    //
    // The stage passes when BOTH halves hold, and the second half is the one that matters:
    // the machinery offers a skeleton and the state machine permits the transition, while
    // the pre-flight still refuses the scaffolded record on every axis. Promotion stays a
    // human act, and the proof demonstrates that by showing the gate holding rather than by
    // walking through it.
    let skeleton = match promote::promotion_skeleton(&finding) {
        Ok(skeleton) => skeleton,
        Err(e) => {
            traversal.stages = stages;
            traversal.verdict = TraversalVerdict::FailedToSurface {
                stage: Stage::Promotable,
                cause: format!("no promotion skeleton could be produced: {e}"),
            };
            return Ok(traversal);
        }
    };
    let refusals = promote::validate_promotion(&finding, &skeleton["behavior"], None, &[]);
    let axes_named = promote::BEHAVIOR_DISPOSITION_AXES.iter().all(|axis| {
        refusals
            .iter()
            .any(|e| matches!(e, PromotionError::MissingDisposition { axis: a, .. } if a == axis))
    });
    if !axes_named {
        traversal.stages = stages;
        traversal.verdict = TraversalVerdict::FailedToSurface {
            stage: Stage::Promotable,
            cause: format!(
                "the promotion pre-flight did not refuse the scaffolded record on every \
                 disposition axis ({refusals:?}); a skeleton that passes validation is a \
                 skeleton a machine could commit"
            ),
        };
        return Ok(traversal);
    }
    if let Err(e) = finding.promote("case-pipeline-proof-not-a-real-case") {
        traversal.stages = stages;
        traversal.verdict = TraversalVerdict::FailedToSurface {
            stage: Stage::Promotable,
            cause: format!("the state machine refused the promotion transition: {e}"),
        };
        return Ok(traversal);
    }
    stages.push(StageOutcome {
        stage: Stage::Promotable,
        detail: format!(
            "the state machine permits promotion and the pre-flight still refuses the \
             scaffold on all {} disposition axes plus its unresolved case — promotion \
             remains a human act",
            promote::BEHAVIOR_DISPOSITION_AXES.len()
        ),
    });

    traversal.stages = stages;
    Ok(traversal)
}

/// Establish a baseline and traverse every declared injection.
///
/// Takes the [`RegressionHarness`] capability by reference so the *call site* shows the
/// process took it out: [`perturb_source`] fails closed without it (FR-070), and a proof
/// that could run without declaring it would be a proof of nothing.
pub async fn run(
    request: &ProofRequest,
    _capability: &RegressionHarness,
) -> Result<ProofReport, HarnessError> {
    let injections = proof_injections()?;
    let ctx = establish(request).await?;
    let mut traversals = Vec::with_capacity(injections.len());
    for record in &injections {
        traversals.push(traverse(&ctx, record).await?);
    }
    Ok(ProofReport::build(
        &request.seed_hex,
        ctx.candidate_id(),
        traversals,
    ))
}

// ---------------------------------------------------------------------------
// The injected comparison
// ---------------------------------------------------------------------------

/// One injected comparison: the differential result plus how many artifacts were perturbed.
struct Injected {
    result: DifferentialResult,
    applied: usize,
}

impl Injected {
    /// The NEW observation on `channel`, if the comparison produced one.
    fn on_declared_channel(&self, channel: &str) -> Option<&Observation> {
        self.result
            .new_observations()
            .find(|o| o.signature.channel == channel)
    }
}

/// Run deacon over `document`, perturb the captured artifact at the **sealed** evidence-source
/// boundary, and relate the perturbed side to the unperturbed one.
///
/// The perturbation is applied to a [`RunContext`] — the raw captured artifact — through
/// [`perturb_source`], whose signature is generic over the sealed
/// [`EvidenceSource`](crate::inject::EvidenceSource). Handing it an observer's output does
/// not compile, so this function cannot plant a difference downstream of the part it is
/// testing even by mistake (research D7).
async fn inject_and_compare(
    ctx: &ProofContext,
    record: &RegressionRecord,
    document: &Value,
) -> Result<Injected, HarnessError> {
    // The candidate's own workspace when the document is the candidate's; a fresh one for a
    // reduction proposal. Materializing the same tree shape either way is load-bearing: a
    // probe workspace whose scaffolding differed would be measuring the scaffold.
    let owned;
    let (workspace, case): (&Path, String) = if document == &ctx.candidate.document {
        (ctx.workspace.path(), ctx.candidate.id.clone())
    } else {
        owned = materialize_document(document).map_err(|e| HarnessError::Report {
            cause: format!("could not materialize a proof probe workspace: {e}"),
        })?;
        (
            owned.path(),
            format!("{}-probe-{}", ctx.candidate.id, record.id),
        )
    };

    // ONE invocation, two sides. The unperturbed capture is the counterpart; the perturbed
    // copy is the side under test. Running deacon twice would introduce a second source of
    // difference into a comparison whose whole value is that it has exactly one.
    //
    // The baseline candidate reuses the evidence `establish` already proved clean, rather
    // than re-running and re-normalizing it: the counterpart every injection is measured
    // against must be the *same* evidence the empty-baseline check passed, or the guarantee
    // that check establishes would not be the guarantee this comparison relies on.
    let (outcome, reference) = if document == &ctx.candidate.document {
        (ctx.baseline_outcome.clone(), ctx.baseline.clone())
    } else {
        let outcome = run_deacon(&ctx.request, &case, workspace).await?;
        let reference = side_evidence(&outcome, workspace, &ctx.request.report_root, &case);
        (outcome, reference)
    };

    let mut source = RunContext::for_side(workspace.to_path_buf(), Side::Deacon);
    source.record_outcome(PROOF_OPERATION, outcome);
    // ===== THE SEALED BOUNDARY (research D7) =====
    let applied = perturb_source(&mut source, record)?;
    let perturbed = source.outcome(PROOF_OPERATION).cloned().ok_or_else(|| {
        HarnessError::InjectionInapplicable {
            record: record.id.clone(),
            cause: "the perturbed run context no longer carries the operation's outcome"
                .to_string(),
        }
    })?;
    let deacon = side_evidence(&perturbed, workspace, &ctx.request.report_root, &case);

    Ok(Injected {
        result: differential::result_from_sides(
            &case,
            deacon,
            reference,
            // Deliberately EMPTY — see the module docs. A tolerance index built for
            // deacon-vs-reference differences could suppress the planted one and report a
            // working pipeline as broken.
            &Characterization::default(),
            false,
        ),
        applied,
    })
}

/// The live reproduction predicate for the proof's minimization stage.
///
/// The same shape as [`super::minimize::DifferentialProbe`] and for the same reason: the
/// shrinker takes its predicate as a parameter (research D4/D5), so the *strategy* stays
/// hermetic while the caller supplies the real one. What differs is only where the
/// counterpart comes from — deacon's own unperturbed run rather than the oracle.
struct InjectedProbe<'a> {
    ctx: &'a ProofContext,
    record: &'a RegressionRecord,
    target: Signature,
    probes: u64,
}

impl ReproductionProbe for InjectedProbe<'_> {
    type Error = HarnessError;

    async fn probe(&mut self, document: &Value) -> Result<Reproduction, HarnessError> {
        self.probes += 1;
        let injected = match inject_and_compare(self.ctx, self.record, document).await {
            Ok(injected) => injected,
            // A proposal the perturbation cannot be applied to has said "this reduction
            // removed the artifact the difference lives in", which is a fact about the
            // PROPOSAL, not about the record — the record already landed on the baseline
            // before minimization began. Rejecting the step is therefore correct, and
            // treating it as a fault would make every reduction that removes the document a
            // reported pipeline defect. This is the same accommodation
            // `DifferentialProbe` makes for a per-probe timeout.
            Err(HarnessError::InjectionInapplicable { cause, .. }) => {
                tracing::debug!(
                    record = %self.record.id,
                    %cause,
                    "a reduction proposal removed the artifact the perturbation targets; the \
                     step is rejected"
                );
                return Ok(Reproduction::Absent);
            }
            Err(other) => return Err(other),
        };

        if injected
            .result
            .observations
            .iter()
            .any(|o| o.signature.id == self.target.id)
        {
            return Ok(Reproduction::Preserved);
        }
        let drifted: Vec<Signature> = injected
            .result
            .new_observations()
            .map(|o| o.signature.clone())
            .collect();
        Ok(if drifted.is_empty() {
            Reproduction::Absent
        } else {
            Reproduction::Drifted(drifted)
        })
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Run deacon's `read-configuration` over `workspace`, capturing raw stdout/stderr.
async fn run_deacon(
    request: &ProofRequest,
    case: &str,
    workspace: &Path,
) -> Result<ProcessOutcome, HarnessError> {
    let folder = workspace.to_string_lossy().into_owned();
    let args: Vec<&str> = vec!["read-configuration", "--workspace-folder", &folder];
    let invocation = run_and_capture(
        Side::Deacon,
        PROOF_BINARY,
        case,
        &request.deacon_binary,
        &args,
        workspace,
        request.bound,
        &request.report_root,
    )
    .await?;
    Ok(ProcessOutcome {
        exit_code: invocation.exit_code,
        success: invocation.success,
        stdout: invocation.stdout.clone(),
        stderr: invocation.stderr.clone(),
        failure_phase: None,
    })
}

/// Build one side's evidence from a raw process outcome.
///
/// **Both sides are normalized as [`Side::Deacon`]**, because both sides ARE deacon. The
/// single normalizer is side-asymmetric on purpose (024 T123), so labelling one copy of
/// deacon's output as the reference would apply rules written for a different serializer and
/// manufacture differences nothing found. The empty-baseline check in [`establish`] is
/// satisfied by construction while this holds — which is the point: it is the guard that
/// would catch a refactor making the two sides asymmetric again, not the argument for the
/// choice.
fn side_evidence(
    outcome: &ProcessOutcome,
    workspace: &Path,
    report_root: &Path,
    case: &str,
) -> SideEvidence {
    let normalized = differential::structured_document_bytes(
        outcome.success,
        &outcome.stdout,
        workspace,
        Side::Deacon,
    );
    let raw_dir = report_root.join("raw").join(PROOF_BINARY).join(case);
    SideEvidence {
        outcome: if outcome.success {
            OutcomeClass::Accepted
        } else {
            OutcomeClass::Rejected
        },
        exit_code: outcome.exit_code,
        stdout_path: raw_dir.join("deacon.stdout"),
        stderr_path: raw_dir.join("deacon.stderr"),
        normalized,
    }
}

/// The candidate parts absent from `dir`, or `None` when all six are present.
fn missing_parts(dir: &Path) -> Option<String> {
    let missing: Vec<&str> = CANDIDATE_PARTS
        .iter()
        .copied()
        .filter(|part| !dir.join(part).exists())
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(missing.join(", "))
    }
}

/// What the comparison DID see, for a failure message that names the shortfall rather than
/// only reporting one.
fn describe(result: &DifferentialResult) -> String {
    if result.observations.is_empty() {
        return "nothing at all".to_string();
    }
    result
        .observations
        .iter()
        .map(|o| {
            format!(
                "{}.{} [{}]",
                o.signature.channel,
                o.signature.path,
                if o.is_new() { "new" } else { "characterized" }
            )
        })
        .collect::<Vec<String>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The proof's own records go through the strict `reg-` loader, so a mis-shaped
    /// perturbation fails here rather than at run time — and every one names a channel the
    /// configuration differential actually reads.
    #[test]
    fn the_declared_injections_load_and_target_channels_the_comparison_reads() {
        let records = proof_injections().expect("the proof's injections load");
        assert_eq!(records.len(), 2);
        let channels: Vec<&str> = records.iter().map(|r| r.channel.as_str()).collect();
        assert!(channels.contains(&"chan-structured-output"));
        assert!(channels.contains(&"chan-exit-code"));
        for record in &records {
            assert!(
                record.notes.is_some(),
                "`{}` must say why its perturbation is a meaningful difference on its \
                 channel",
                record.id
            );
        }
    }

    /// The status rule, asserted against the same function the bin calls.
    #[test]
    fn the_exit_status_distinguishes_all_four_outcomes() {
        let traversed = |id: &str| Traversal {
            injection: id.to_string(),
            channel: "chan-exit-code".to_string(),
            applied: 1,
            signature: Some("sig-1".to_string()),
            finding: Some("fnd-1".to_string()),
            stages: Vec::new(),
            verdict: TraversalVerdict::Traversed,
        };
        let mut failed = traversed("reg-b");
        failed.verdict = TraversalVerdict::FailedToSurface {
            stage: Stage::Comparison,
            cause: "swallowed".to_string(),
        };
        let mut inapplicable = traversed("reg-c");
        inapplicable.verdict = TraversalVerdict::InjectionInapplicable {
            cause: "never landed".to_string(),
        };

        assert_eq!(
            ProofReport::build("0x1", "cnd-1", vec![traversed("reg-a")]).exit_status(),
            0
        );
        assert_eq!(
            ProofReport::build("0x1", "cnd-1", vec![traversed("reg-a"), failed]).exit_status(),
            1,
            "a difference that landed and did not traverse is a pipeline defect"
        );
        assert_eq!(
            ProofReport::build("0x1", "cnd-1", vec![traversed("reg-a"), inapplicable])
                .exit_status(),
            1,
            "an injection that never landed must FAIL, never count as found-nothing"
        );
        assert_eq!(
            ProofReport::build("0x1", "cnd-1", Vec::new()).exit_status(),
            1,
            "a proof over zero injections reports exactly what a fully-passing one does"
        );
    }

    #[test]
    fn the_report_renders_byte_stably_with_the_verdict_flattened() {
        let report = ProofReport::build(
            "0xseed",
            "cnd-11111111",
            vec![Traversal {
                injection: "reg-proof-exit-code".to_string(),
                channel: "chan-exit-code".to_string(),
                applied: 1,
                signature: Some("sig-abcd1234".to_string()),
                finding: Some("fnd-abcd1234".to_string()),
                stages: vec![StageOutcome {
                    stage: Stage::Comparison,
                    detail: "surfaced".to_string(),
                }],
                verdict: TraversalVerdict::Traversed,
            }],
        );
        let rendered = report.render().expect("renders");
        assert!(rendered.ends_with('\n'));
        assert!(rendered.contains("\"verdict\": \"traversed\""));
        assert_eq!(rendered, report.render().expect("renders"));
        let round_tripped: ProofReport =
            serde_json::from_str(&rendered).expect("the report round-trips");
        assert_eq!(round_tripped, report);
    }

    #[test]
    fn every_stage_has_a_stable_spelling_and_the_list_is_the_traversal_order() {
        assert_eq!(
            Stage::all()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>(),
            vec![
                "generation",
                "comparison",
                "minimization",
                "candidate",
                "classification",
                "promotable",
            ],
            "FR-042a names these stages in this order; the report is read against that list"
        );
    }
}
