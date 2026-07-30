//! Campaign driver: seed, tier, budget, per-candidate timeout, admission cap, and
//! outcome accumulation (025-exploratory-parity-discovery, T033/T037,
//! FR-001 – FR-005, FR-011/FR-012, FR-034b, FR-062).
//!
//! The driver owns the four tiers and their prerequisites (research D10): `metamorphic`
//! needs nothing external, `config-differential` is the nightly scheduled tier,
//! `container-differential` is invoked-only, and `corpus` is the weekly network-backed
//! tier. Budgets are per-tier rather than shared, because sharing lets the slow tier
//! starve the fast one — and the fast tier is where nearly all the exploration happens.
//!
//! This module drives all four tiers: the two differential tiers, the `metamorphic` tier
//! (T096), and the network-backed `corpus` tier (T108, US7) — and, since US2 (T052),
//! minimizes each new finding and emits its reviewable candidate.
//!
//! ## What minimization costs, and the two gates on paying it
//!
//! A shrink probe is two CLI invocations, so it is the most expensive thing a campaign
//! does per unit of information. The driver therefore reduces a finding only when both
//! hold:
//!
//! 1. **The signature is new to this campaign.** A finding the queue already carries has a
//!    reduced input recorded against it from the campaign that admitted it; re-deriving it
//!    would spend a full reduction to arrive at the same document.
//! 2. **The wall clock has not run out.** Minimization is not the place to overrun a budget
//!    the candidate loop above already respects.
//!
//! Neither gate may quietly present an unreduced input as minimal: both route through
//! `Reduction::not_attempted`, which carries `isMinimal: false` **and** the reason (FR-022).
//!
//! ## Three shapes of campaign, one record
//!
//! [`run`] dispatches the metamorphic tier **before** acquiring any prerequisite, because
//! it has none to acquire (research D12): it compares deacon against deacon over a declared
//! transformation, so there is no oracle to verify, no Docker to probe, and no network to
//! reach. Routing it through the differential's prerequisite step would make the one tier a
//! contributor can run with nothing installed depend on everything.
//!
//! The `corpus` tier dispatches **after** prerequisites, because it needs both the verified
//! oracle and the network — it is the same deacon-vs-reference comparison the differential
//! runs, over inputs nobody in this repository wrote. Its inputs are pinned third-party
//! snapshots rather than generated documents, so it draws no candidates and applies no
//! mutations; everything downstream of the comparison is shared.
//!
//! All three shapes produce the **same** [`Campaign`] / [`CampaignOutcomeReport`] record and
//! admit through the same [`AdmissionQueue`], so a reader of `campaigns.json` does not need
//! to know which driver wrote a row, and the admission cap means the same thing on all of
//! them.
//!
//! ## What a campaign's exit status means
//!
//! It reflects **whether it ran**, never **what it found** (contracts/discovery-cli.md,
//! FR-058): forty differences exit `0`; an unverifiable oracle exits non-zero. Any command
//! whose status depends on its findings becomes a gate the moment someone wires it into
//! CI, and a stochastic gate makes green non-reproducible.
//!
//! ## Prerequisites fail loudly, never silently (FR-003)
//!
//! A missing or wrong-version oracle is [`HarnessError::OracleUnverified`] naming the
//! cause, and the campaign reports **no findings at all** — not an empty set, which would
//! be indistinguishable from agreement. The same for Docker on the container-backed tier.
//! There is no skip path.
//!
//! ## Two guards on what may be executed
//!
//! - **FR-011** — a candidate that cannot be executed within the declared safety
//!   constraints is **discarded and counted**, never executed. The predicates are hermetic
//!   (`deacon_conformance::discovery::generate::unsafe_reasons`) so they are testable
//!   without a campaign; the driver's only job is to discard and count.
//! - **FR-012** — a container-bound candidate referencing an unpinned image is discarded
//!   the same way. An unpinned input makes the comparison non-reproducible in the one way
//!   the pinned input set cannot record: `alpine:latest` is a different image tomorrow, so
//!   a finding recorded against it is a claim about content nobody can retrieve.
//!
//! Both are counted into `candidatesDiscardedUnsafe`, so a *rising* discard rate is
//! visible rather than silently shrinking the explored space.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use deacon_conformance::discovery::generate::{
    Candidate, Generator, required_keys, unpinned_image_inputs, unsafe_reasons,
};
use deacon_conformance::discovery::grammar::Grammar;
use deacon_conformance::discovery::mutate::{self, ApplicationCounts, MUTATION_CATALOG_VERSION};
use deacon_conformance::discovery::queue::{
    Budget, Campaign, CampaignLane, CampaignOutcome, CampaignTier, DiscoveryData, Finding,
    ObservedValues, PinnedInputSet, Witness, upsert_finding, write_campaigns, write_findings,
};
use deacon_conformance::discovery::report::{CampaignOutcomeReport, build_campaign_outcome_report};
use deacon_conformance::discovery::shrink::{self, ReductionInput};
use deacon_conformance::discovery::signature::Signature;
use deacon_conformance::load::Registry;
use serde_json::Value;

use crate::HarnessError;
use crate::normalize::NORMALIZER_VERSION;
use crate::oracle::{Oracle, OraclePin, VerifiedOracle};
use crate::prereq;

use super::corpus_fetch::{self, EntryStatus};
use super::differential::{self, Characterization, DifferentialInput};
use super::metamorphic_run::{self, Sabotage};
use super::{candidate, minimize};

/// The channel a metamorphic residual is keyed under.
///
/// The evidence document a relation compares spans both declared channel families —
/// `chan-exit-code` at `exitCode`, `chan-structured-output` under `structuredOutput` — but a
/// [`Signature`] carries exactly one channel, and every residual a relation reports is a
/// difference in the *resolved configuration document*. Keying them all here matches
/// `discovery_metamorphic`'s own use of the same channel, so a metamorphic finding and a
/// differential finding at the same path deduplicate against each other — which is correct:
/// they are the same defect observed two ways.
const METAMORPHIC_CHANNEL: &str = "chan-structured-output";

/// Everything one campaign needs, resolved by the caller (the bin, or a test).
///
/// Every knob is explicit rather than read from the environment, so a test can drive a
/// short bounded campaign without mutating process-global state — which is `unsafe` under
/// this workspace's edition and hostile to a parallel runner besides. It is the same
/// explicit-seam discipline [`crate::prereq::probe_docker`] and
/// [`crate::oracle::verify_binary`] already establish.
#[derive(Debug, Clone)]
pub struct CampaignRequest {
    /// The seed, exactly as recorded (FR-001). **Never defaulted** at the CLI.
    pub seed_hex: String,
    /// The seed's numeric value, feeding the generator's stream.
    pub seed: u64,
    /// Which tier to run.
    pub tier: CampaignTier,
    /// Which lane invoked it.
    pub lane: CampaignLane,
    /// The certification profile the run happens under (a `prof-` id).
    pub profile: String,
    /// The declared budget.
    pub budget: Budget,
    /// How many candidates the run **plans** to reach. The denominator of
    /// `spaceCoveredFraction`: on budget exhaustion the campaign reports the portion of
    /// its plan it covered rather than presenting a truncated run as complete (FR-005).
    pub planned_candidates: u64,
    /// The registry root, for resolving pins, channels, and the tolerance index.
    pub registry_dir: PathBuf,
    /// The discovery data root the queue is read from and written to.
    pub discovery_dir: PathBuf,
    /// Where raw artifacts are written (`target/discovery` by default).
    pub report_root: PathBuf,
    /// The deacon binary under test.
    pub deacon_binary: PathBuf,
    /// An explicit oracle binary, bypassing `PATH` resolution. The verification seam: a
    /// test can point this at a stub reporting the wrong version and observe the fail-loud
    /// path without touching process env.
    pub oracle_override: Option<PathBuf>,
    /// Whether to persist the campaign and its findings. `false` leaves the committed data
    /// root untouched, which is what an acceptance test wants.
    pub persist: bool,
}

impl CampaignRequest {
    /// Whether this tier brings containers up, which decides which safety predicates apply
    /// (FR-011) and whether the pinned-image guard is in force (FR-012).
    pub fn container_backed(&self) -> bool {
        self.tier == CampaignTier::ContainerDifferential
    }
}

/// What a campaign produced.
#[derive(Debug, Clone)]
pub struct CampaignRun {
    /// The provenance record, ready to append to `campaigns.json`.
    pub campaign: Campaign,
    /// The findings queue after this campaign's upserts.
    pub findings: Vec<Finding>,
    /// The ids this campaign admitted or re-witnessed, in admission order.
    pub admitted: Vec<String>,
    /// The campaign's own report (FR-061).
    pub report: CampaignOutcomeReport,
    /// How many observations were already characterized by a case or waiver (FR-017).
    ///
    /// Not part of the record — the record's counters are the declared ones — but reported
    /// so a reader can tell "we saw nothing" from "we saw only what we already knew".
    pub characterized_observations: u64,
    /// Probes minimization spent across every finding (T052).
    ///
    /// Also not part of the record: the *cost* of reduction is a property of the run, not
    /// of the findings it produced, and a witness already carries whether its own input
    /// reached minimality. Reported so a campaign can say what reduction cost it rather
    /// than leaving it folded invisibly into the wall clock.
    pub shrink_probes: u64,
    /// Where the reviewable candidates were written (`<report_root>/candidates`).
    pub candidates_root: PathBuf,
    /// The findings a reviewable candidate was emitted for, in admission order.
    ///
    /// Deliberately **not** every finding in the queue. A finding admitted from a
    /// *comparison* has a full `DifferentialResult` behind it — both sides' raw evidence,
    /// both normalized documents, the diff — which is what FR-024's six parts are made of.
    /// A finding admitted from a minimization *drift* (FR-023) does not: it was seen in
    /// passing by a probe while reducing something else, and the campaign kept its
    /// signature and the input that reproduces it, not a full evidence set.
    ///
    /// Reported separately rather than papered over. Assembling a six-part candidate for a
    /// drift would mean either fabricating the parts we did not gather or spending another
    /// pair of CLI invocations per drift; leaving the queue record without a candidate and
    /// *saying so* is the honest third option — the drift is a lead, and the campaign that
    /// generates a candidate reproducing it will evidence it properly.
    pub candidates: Vec<String>,
    /// Per-entry outcomes of the `corpus` tier, empty for every other tier (FR-051/FR-052).
    ///
    /// Reported alongside the counters rather than folded into them because the two
    /// non-comparing outcomes are *different facts*: an unreachable entry says nothing was
    /// compared, a digest mismatch says the upstream snapshot is not what was recorded.
    /// A single "did not run" tally would let content drift at a pinned commit read as a
    /// flaky network, which is the one confusion an ecological canary cannot afford.
    pub corpus_statuses: Vec<EntryStatus>,
}

/// Run one campaign.
///
/// Fails loudly on any prerequisite problem and reports **no findings** in that case
/// (FR-003): an empty finding set from an unverified reference would be indistinguishable
/// from agreement, which is the single most comfortable way for this machinery to be
/// broken.
pub async fn run(request: &CampaignRequest) -> Result<CampaignRun, HarnessError> {
    let registry = Registry::load(&request.registry_dir).map_err(|e| HarnessError::Report {
        cause: format!(
            "could not load the conformance registry at {}: {e}",
            request.registry_dir.display()
        ),
    })?;
    let grammar = Grammar::load_default().map_err(|e| HarnessError::Report {
        cause: format!("could not load the generation grammar: {e}"),
    })?;

    // The metamorphic tier branches BEFORE any prerequisite is acquired: it has none
    // (research D12). Verifying an oracle it never invokes would make the one tier a
    // contributor can run with nothing installed fail for the absence of a reference that
    // takes no part in its comparison.
    if request.tier == CampaignTier::Metamorphic {
        return run_metamorphic(request, &registry, &grammar).await;
    }

    // Prerequisites first, so a campaign never produces a partial record against an
    // unverified reference.
    let oracle = if request.tier.requires_oracle() {
        Some(verified_oracle(request).await?)
    } else {
        None
    };
    if request.container_backed() {
        prereq::require_docker().await?;
    }
    if request.tier == CampaignTier::Corpus {
        // The network is this tier's prerequisite, and it fails as loudly as a missing
        // oracle: a corpus campaign with no network reports zero entries, which is
        // byte-identical to one in which the whole ecosystem agreed with deacon.
        corpus_fetch::require_git().await?;
        let Some(oracle) = oracle.as_ref() else {
            return Err(HarnessError::OracleUnverified {
                cause: "the corpus tier reached its driver with no verified oracle".to_string(),
            });
        };
        return run_corpus(request, &registry, &grammar, oracle).await;
    }

    let pinned_input_set = pinned_input_set(&grammar, oracle.as_ref())?;
    let campaign_id = Campaign::derive_id(
        &request.seed_hex,
        &pinned_input_set,
        request.lane,
        &request.profile,
        request.tier,
    );

    let characterization = Characterization::from_registry(&registry);
    if characterization.is_empty() {
        tracing::warn!(
            campaign = %campaign_id,
            "the tolerance index is empty: every already-characterized divergence will be \
             reported as new"
        );
    }

    // The standing queue, so deduplication spans campaigns (FR-030/FR-034).
    let existing =
        DiscoveryData::load(&request.discovery_dir).map_err(|e| HarnessError::Report {
            cause: format!(
                "could not load the discovery data root at {}: {e}",
                request.discovery_dir.display()
            ),
        })?;
    let mut queue = AdmissionQueue::new(
        &existing.findings,
        &campaign_id,
        request.budget.admission_cap,
    );

    let mut generator = Generator::new(&grammar, request.seed);
    let mut counters = Counters::new();
    let mut observed: BTreeSet<String> = BTreeSet::new();

    let wall_clock = Duration::from_secs(request.budget.wall_clock_seconds);
    let per_candidate = Duration::from_secs(request.budget.per_candidate_seconds);
    let started = Instant::now();
    let candidates_root = request.report_root.join("candidates");
    let mut candidates: Vec<String> = Vec::new();

    while counters.generated < request.planned_candidates {
        if started.elapsed() >= wall_clock {
            break;
        }
        let candidate = generator.next_candidate();
        counters.generated += 1;
        for mutation in &candidate.mutations {
            if let Some(slot) = counters.mutations.get_mut(mutation.category.name()) {
                *slot += 1;
            }
        }

        // FR-011 / FR-012: discard and count, never execute.
        let mut refusals = unsafe_reasons(&candidate.document, request.container_backed());
        if request.container_backed() {
            for image in unpinned_image_inputs(&candidate.document) {
                refusals.push(format!(
                    "references the unpinned image input `{image}`, which is a different \
                     image tomorrow"
                ));
            }
        }
        if !refusals.is_empty() {
            counters.discarded_unsafe += 1;
            tracing::debug!(
                candidate = %candidate.id,
                reasons = ?refusals,
                "discarded an unsafe candidate before execution"
            );
            continue;
        }

        let Some(oracle) = oracle.as_ref() else {
            // Only the metamorphic tier has no oracle, and [`run`] now routes it to
            // [`run_metamorphic`] before this loop is reached (T096). Arriving here would
            // mean a tier was routed into the differential without its prerequisite — which
            // must fail rather than compare against nothing.
            return Err(HarnessError::OracleUnverified {
                cause: format!(
                    "tier `{}` reached the differential with no verified oracle",
                    request.tier.as_str()
                ),
            });
        };

        let workspace = match materialize(&candidate) {
            Ok(w) => w,
            Err(e) => {
                counters.discarded_unsafe += 1;
                tracing::warn!(
                    candidate = %candidate.id,
                    error = %e,
                    "could not materialize a candidate workspace"
                );
                continue;
            }
        };

        let result = differential::compare(
            DifferentialInput {
                candidate_id: &candidate.id,
                workspace: workspace.path(),
                deacon: &request.deacon_binary,
                oracle,
                bound: per_candidate,
                report_root: &request.report_root,
                // A near-valid draw violates a `required` key on purpose, and a mutated
                // document was made adjacent-to-valid on purpose. Either way the candidate
                // is deliberately malformed, which is the scope the strictness waivers
                // characterize — a plain grammar-valid draw is NOT, so deacon refusing one
                // stays news (FR-017 read narrowly, on purpose).
                deliberately_invalid: candidate.kind
                    == deacon_conformance::discovery::generate::CandidateKind::NearValid
                    || !candidate.mutations.is_empty(),
            },
            &characterization,
        )
        .await;

        let result = match result {
            Ok(r) => r,
            Err(HarnessError::OracleTimeout { .. }) => {
                // One pathological input must not consume the tier's whole budget. It is
                // discarded and COUNTED, so a rising rate is visible rather than silent.
                counters.timed_out += 1;
                tracing::debug!(
                    candidate = %candidate.id,
                    "candidate exceeded its per-candidate bound"
                );
                continue;
            }
            Err(other) => return Err(other),
        };

        counters.executed += 1;
        if result.parse_stage_failure {
            counters.parse_stage_failures += 1;
        }
        counters.characterized += result.characterized_count() as u64;

        for observation in &result.observations {
            observed.insert(observation.signature.id.clone());
        }

        // The reduction's starting point, shared by every finding this candidate produced:
        // the same document, the same recorded operators, and the branch's `required` keys
        // read from the grammar rather than restated (research D1).
        let reduction_input = ReductionInput {
            document: candidate.document.clone(),
            mutations: candidate.mutations.clone(),
            required_keys: candidate
                .branch
                .map(|branch| required_keys(&grammar, branch))
                .unwrap_or_default(),
        };
        let deliberately_invalid = candidate.kind
            == deacon_conformance::discovery::generate::CandidateKind::NearValid
            || !candidate.mutations.is_empty();

        for observation in result.new_observations() {
            let finding_id = observation.signature.finding_id();

            // T052 — minimize, under a per-finding shrink budget.
            //
            // Two gates, both about not spending the expensive step on something already
            // known. A finding the queue already carries has a reduced input recorded
            // against it from the campaign that admitted it, and re-deriving it would cost
            // a full reduction to reach the same document. A campaign past its wall clock
            // has stopped exploring, and minimization is not the place to overrun a budget
            // the loop above already respects.
            //
            // Neither gate may present the result as minimal: both route through
            // `Reduction::not_attempted`, which carries the reason (FR-022).
            let reduction = if !queue.is_new(&finding_id) {
                shrink::Reduction::not_attempted(
                    &reduction_input,
                    "the findings queue already carries this signature, and the reduced \
                     input recorded when it was admitted still stands",
                )
            } else if started.elapsed() >= wall_clock {
                shrink::Reduction::not_attempted(
                    &reduction_input,
                    "the campaign's wall-clock budget was exhausted before minimization \
                     could run",
                )
            } else {
                let mut probe = minimize::DifferentialProbe::new(
                    observation.signature.clone(),
                    &candidate.id,
                    &request.deacon_binary,
                    oracle,
                    per_candidate,
                    &request.report_root,
                    &characterization,
                    deliberately_invalid,
                );
                shrink::reduce(
                    &reduction_input,
                    request.budget.shrink_steps_per_finding,
                    &mut probe,
                )
                .await?
            };
            counters.shrink_probes += reduction.probes;

            let witness = Witness {
                id: Witness::derived_id(&campaign_id, &candidate.id),
                campaign_id: campaign_id.clone(),
                candidate_id: candidate.id.clone(),
                minimal_input: reduction.document.clone(),
                // Never asserted, always reported: `true` only when a complete pass over
                // the seven-step catalogue preserved nothing (FR-021), and `false` carries
                // its reason (FR-022).
                is_minimal: reduction.is_minimal,
                reduction_steps: reduction.steps.clone(),
                observed_values: ObservedValues {
                    deacon: observation.observed.deacon.clone(),
                    reference: observation.observed.reference.clone(),
                },
                mutation_operators: candidate.operator_ids(),
            };
            queue.offer(observation.signature.clone(), witness);

            // FR-023 — a step that changed the signature was rejected for THIS finding, and
            // what it produced instead is a candidate finding in its own right. Admitting
            // it here rather than waiting for a later campaign to rediscover it is the
            // difference between observing a difference and remembering it.
            for drifted in &reduction.drifted {
                observed.insert(drifted.signature.id.clone());
                let drift_witness = Witness {
                    // Derived from the CANDIDATE, like every other witness this crate
                    // writes — never from the drifted signature. A witness id is
                    // substance-anchored over `campaignId ‖ candidateId` (D1 recomputes it
                    // from those two stored fields), so hashing anything else produces a
                    // record the discovery loader rejects as malformed. It did: the first
                    // real config-differential campaign wrote 14 such witnesses, and every
                    // one of them failed `discovery check`, which is why no finding had
                    // ever been committed to the queue.
                    //
                    // Two drifts from ONE candidate into ONE finding therefore collapse to
                    // a single witness (`upsert_finding` returns `AlreadyWitnessed`). That
                    // is the intended reading, not a loss: the id says an observation is
                    // identified by the campaign and the input that produced it, and both
                    // drifts came from the same input. The drifted signature still gets its
                    // own FINDING — which is what FR-023 is about.
                    id: Witness::derived_id(&campaign_id, &candidate.id),
                    campaign_id: campaign_id.clone(),
                    candidate_id: candidate.id.clone(),
                    // The rejected proposal the drift was SEEN on — not the candidate's
                    // document and not the reduction's result, neither of which produces
                    // this signature. A witness naming an input that does not reproduce
                    // its own signature is a record nobody can re-examine.
                    minimal_input: drifted.document.clone(),
                    // It was never itself reduced; it is a by-product of reducing
                    // something else, and saying otherwise would claim work nobody did.
                    is_minimal: false,
                    reduction_steps: Vec::new(),
                    observed_values: ObservedValues::default(),
                    mutation_operators: candidate.operator_ids(),
                };
                queue.offer(drifted.signature.clone(), drift_witness);
            }

            // The reviewable candidate (FR-024 – FR-027). Emitted only for a finding the
            // queue actually holds: a candidate directory for a signature the admission cap
            // turned away would be a reviewable artifact for a finding nobody can find.
            if queue.holds(&finding_id) {
                let dir = candidate::write(candidate::CandidateInputs {
                    finding_id: &finding_id,
                    signature: &observation.signature,
                    observation,
                    campaign_id: &campaign_id,
                    seed_hex: &request.seed_hex,
                    lane: request.lane.as_str(),
                    tier: request.tier.as_str(),
                    profile: &request.profile,
                    pinned_input_set: &pinned_input_set,
                    candidate_id: &candidate.id,
                    operations: &candidate.operations,
                    mutation_operators: &candidate.operator_ids(),
                    reduction: &reduction,
                    result: &result,
                    reference: candidate::ReferenceProvenance::Oracle(oracle),
                    registry: &registry,
                    root: &candidates_root,
                })?;
                if !candidates.contains(&finding_id) {
                    candidates.push(finding_id.clone());
                }
                tracing::debug!(finding = %finding_id, path = %dir.display(), "wrote a reviewable candidate");
            }
        }

        drop(workspace);
    }

    complete(
        request,
        &existing.campaigns,
        Completion {
            campaign_id,
            pinned_input_set,
            counters,
            observed,
            queue,
            planned: request.planned_candidates,
            candidates,
        },
    )
}

/// The `metamorphic` tier: deacon against deacon over the declared relation catalogue
/// (T096, FR-044 – FR-048, research D12).
///
/// # No prerequisite, by construction
///
/// No oracle, no Docker, no network. That is not an optimization — it is what makes this
/// the one complete vertical slice a contributor with nothing installed can run, and it is
/// why [`run`] dispatches here *before* the prerequisite step rather than inside it.
///
/// # The plan IS the catalogue
///
/// There is nothing to generate for this tier: the relations are hand-authored registry
/// records, so [`CampaignRequest::planned_candidates`] — a *generator* denominator — has
/// nothing to denominate. The plan is therefore the catalogue's own size, which makes
/// `spaceCoveredFraction` read `1.0` when every relation was reached and less **only** when
/// the wall clock cut the run short. Using the request's figure would report a seven-relation
/// tier as having covered 3.5% of a two-hundred-candidate plan it never had.
///
/// `mutationApplications` is all-zero for the same reason (no mutation occurs here, and
/// FR-010 requires every category to be present as an explicit zero rather than absent), and
/// `parseStageFailures` is zero because there is no document-syntax stage to fail: the
/// fixtures are authored, not drawn.
///
/// # What is deliberately NOT wired in
///
/// [`Characterization`] — the FR-017 already-characterized suppression the differential
/// applies — is **not** consulted here. That index is keyed to differential-style observable
/// paths drawn from `cases/<area>.json` and the waivers, both of which describe a
/// deacon-vs-reference difference. A metamorphic residual is a deacon-vs-deacon difference
/// at a path the index says nothing about, so consulting it would either suppress nothing
/// (harmless but dishonest bookkeeping) or suppress by path collision (silently dropping a
/// real finding). Wiring it correctly needs a tolerance model this tier does not yet have.
/// Left out visibly rather than approximated.
async fn run_metamorphic(
    request: &CampaignRequest,
    registry: &Registry,
    grammar: &Grammar,
) -> Result<CampaignRun, HarnessError> {
    let catalogue = &registry.metamorphic;
    if catalogue.is_empty() {
        // Not a zero-relation campaign: V32 already forbids a mandated family with no
        // record, so an empty catalogue here means the registry did not load what it should
        // have. Reporting it as a clean run of nothing would be byte-identical to a run in
        // which every relation held — the exact indistinguishability SC-011 exists to
        // prevent.
        return Err(HarnessError::Report {
            cause: format!(
                "the metamorphic relation catalogue at {} is empty. A campaign over zero \
                 relations reports the same thing as one in which every relation held, so \
                 it is refused rather than run.",
                request.registry_dir.join("metamorphic.json").display()
            ),
        });
    }

    let pinned_input_set = pinned_input_set(grammar, None)?;
    let campaign_id = Campaign::derive_id(
        &request.seed_hex,
        &pinned_input_set,
        request.lane,
        &request.profile,
        request.tier,
    );

    // The standing queue, so deduplication spans campaigns AND tiers (FR-030/FR-034): a
    // metamorphic residual and a differential divergence at the same path derive the same
    // signature, and they are the same defect observed two ways.
    let existing =
        DiscoveryData::load(&request.discovery_dir).map_err(|e| HarnessError::Report {
            cause: format!(
                "could not load the discovery data root at {}: {e}",
                request.discovery_dir.display()
            ),
        })?;
    let mut queue = AdmissionQueue::new(
        &existing.findings,
        &campaign_id,
        request.budget.admission_cap,
    );

    let mut counters = Counters::new();
    let mut observed: BTreeSet<String> = BTreeSet::new();

    let root = request.report_root.join("metamorphic").join(&campaign_id);
    let wall_clock = Duration::from_secs(request.budget.wall_clock_seconds);
    let per_candidate = Duration::from_secs(request.budget.per_candidate_seconds);
    let started = Instant::now();

    for (index, relation) in catalogue.iter().enumerate() {
        if started.elapsed() >= wall_clock {
            break;
        }
        counters.generated += 1;

        // An index prefix so the layout is stable and readable after a failure — the same
        // scheme `evaluate_catalogue` uses, kept identical so a reviewer following a
        // campaign's artifacts finds the tree they expect.
        let relation_root = root.join(format!("{index:02}-{}", relation.id));
        let evaluation = metamorphic_run::evaluate(
            &request.deacon_binary,
            &relation_root,
            relation,
            // The honest evaluation. `Sabotage::Break` is the SC-011 anti-inert probe, and
            // a live campaign that ran it would manufacture findings out of its own
            // deliberate breakage.
            Sabotage::None,
        );
        let outcome = match tokio::time::timeout(per_candidate, evaluation).await {
            Ok(Ok(outcome)) => outcome,
            // A relation the harness cannot apply is fail-loud, never a skip: reporting
            // nothing for it is byte-identical to it holding.
            Ok(Err(e)) => return Err(e),
            Err(_elapsed) => {
                // One pathological relation must not consume the tier's whole budget. It is
                // discarded and COUNTED, so a rising rate is visible rather than silent.
                counters.timed_out += 1;
                tracing::debug!(
                    relation = %relation.id,
                    "relation exceeded its per-candidate bound"
                );
                continue;
            }
        };
        counters.executed += 1;

        let Some(candidate) = outcome.candidate() else {
            continue;
        };
        let signatures = candidate.signatures(METAMORPHIC_CHANNEL);
        if signatures.is_empty() {
            // A SENSITIVITY failure is the absence of a difference, so there is nothing to
            // key a signature on, and the catalogue deliberately refuses to invent one (it
            // would collide with a genuine value difference at the touched site and merge
            // two unrelated defects). The violation is still REPORTED here rather than
            // dropped in silence.
            tracing::warn!(
                relation = %relation.id,
                effect = %outcome.effect.as_str(),
                transformation = %outcome.transformation,
                "relation was VIOLATED but carries no deduplication key, so it cannot enter \
                 the findings queue; it is identified by its relation id"
            );
            continue;
        }

        // The reviewable candidate — both inputs and both normalized outputs — is the
        // witness's input. Recording only one side would name an input that does not
        // reproduce the observation: a metamorphic input is a PAIR.
        let evidence = serde_json::to_value(&candidate).map_err(|e| HarnessError::Report {
            cause: format!(
                "could not record the metamorphic candidate for `{}`: {e}",
                relation.id
            ),
        })?;

        for signature in signatures {
            observed.insert(signature.id.clone());
            // Paired by observable path rather than by position: `signatures()` filters,
            // so an index-aligned zip would silently mis-attribute values the moment one
            // residual failed to classify.
            let residual = candidate.residual.iter().find(|r| r.path == signature.path);
            let witness = Witness {
                id: Witness::derived_id(&campaign_id, &relation.id),
                campaign_id: campaign_id.clone(),
                // The relation id IS the candidate id for this tier: the input is not drawn
                // from a stream, it is the relation's declared base fixture plus its
                // declared transformation, and naming it anything else would lose the one
                // thing that reproduces the observation.
                candidate_id: relation.id.clone(),
                minimal_input: evidence.clone(),
                // No reduction is performed, so the input is NOT minimal and says so
                // (FR-022). A relation's fixture is already small by construction; that is
                // not the same claim as having been reduced.
                is_minimal: false,
                reduction_steps: Vec::new(),
                observed_values: ObservedValues {
                    // `reference` is the original run and `deacon` the transformed one —
                    // the mapping `MetamorphicCandidate::signatures` fixes, restated here
                    // so the two views cannot drift apart.
                    deacon: residual.and_then(|r| r.transformed.clone()),
                    reference: residual.and_then(|r| r.original.clone()),
                },
                // No mutation operator produced this input; the transformation did, and it
                // is named in the candidate the witness carries.
                mutation_operators: Vec::new(),
            };
            queue.offer(signature, witness);
        }
    }

    complete(
        request,
        &existing.campaigns,
        Completion {
            campaign_id,
            pinned_input_set,
            counters,
            observed,
            queue,
            // The plan is the catalogue — see this function's docs.
            planned: catalogue.len() as u64,
            // No reviewable candidate is packaged here: FR-024's six parts are built from a
            // deacon-vs-reference evidence set, and this tier compares deacon against
            // deacon. The relation's own artifacts under `target/discovery/metamorphic/`
            // are its review surface.
            candidates: Vec::new(),
        },
    )
}

/// The `corpus` tier: deacon against the verified pinned oracle over the pinned real-world
/// workspace corpus (T108, FR-049 – FR-054a, research D8/D10).
///
/// # An ecological canary, not a generator
///
/// The inputs are 33 third-party workspace snapshots nobody in this repository wrote, so
/// nothing is drawn and nothing is mutated: `mutationApplications` is all-zero (present as
/// explicit zeroes, FR-010) and the plan is the manifest's own size, exactly as the
/// metamorphic tier's plan is its catalogue's. Using the request's `planned_candidates`
/// would report a 33-entry tier as having covered 16% of a plan it never had.
///
/// # The corpus is never a mutation seed (FR-008a / FR-054a)
///
/// It is consumed **here**, as a direct comparison input, and nowhere else. The generator's
/// seed corpus is five committed fixtures embedded with `include_str!`
/// (`deacon_conformance::discovery::generate`), so a corpus entry cannot reach it even by
/// accident: there is no code path from a fetched workspace into the generator, and the
/// seeds are fixed at compile time rather than discovered at run time.
///
/// # Two ways an entry does not get compared, and they are never merged
///
/// - **Unreachable** (FR-052) — the snapshot could not be retrieved, or the pinned path
///   carries no devcontainer configuration at that commit. Nothing was compared.
/// - **Digest mismatch** (FR-051) — the snapshot was retrieved and is not what the
///   manifest recorded. It is deliberately *not* compared: attributing a change in the
///   upstream workspace to a difference between the implementations is exactly the wrong
///   conclusion, and it is the one a tolerant fetch would invite.
///
/// Both are counted into `candidatesDiscardedUnsafe` — the record's "generated but
/// deliberately not executed" tally, which already carries the differential's timeouts —
/// and both are named individually on [`CampaignRun::corpus_statuses`], because the
/// aggregate number cannot tell a reviewer *which* fact occurred.
///
/// # Everything after the comparison is shared
///
/// FR-054 requires a corpus finding to enter the same pipeline as a generated one, and it
/// does so by construction rather than by parallel implementation: the same
/// [`differential::compare`], the same [`Characterization`] tolerance index, the same
/// [`AdmissionQueue`] deduplication and cap, and the same [`complete`]. The only
/// corpus-specific thing about a corpus finding is that its witness's `minimalInput` names
/// the upstream provenance — repository, commit, path, and the verified digest — because a
/// witness whose input nobody can retrieve names nothing.
async fn run_corpus(
    request: &CampaignRequest,
    registry: &Registry,
    grammar: &Grammar,
    oracle: &VerifiedOracle,
) -> Result<CampaignRun, HarnessError> {
    let pinned_input_set = pinned_input_set(grammar, Some(oracle))?;
    let campaign_id = Campaign::derive_id(
        &request.seed_hex,
        &pinned_input_set,
        request.lane,
        &request.profile,
        request.tier,
    );

    let existing =
        DiscoveryData::load(&request.discovery_dir).map_err(|e| HarnessError::Report {
            cause: format!(
                "could not load the discovery data root at {}: {e}",
                request.discovery_dir.display()
            ),
        })?;

    if existing.corpus.is_empty() {
        // Not a zero-entry campaign. An empty manifest here means the data root did not
        // load what it should have, and reporting it as a clean run of nothing would be
        // byte-identical to a run in which every pinned workspace agreed — the exact
        // indistinguishability this tier exists to prevent.
        return Err(HarnessError::Report {
            cause: format!(
                "the corpus manifest at {} is empty. A campaign over zero entries reports \
                 the same thing as one in which every entry agreed, so it is refused \
                 rather than run.",
                deacon_conformance::discovery::queue::corpus_path(&request.discovery_dir).display()
            ),
        });
    }

    let characterization = Characterization::from_registry(registry);
    if characterization.is_empty() {
        tracing::warn!(
            campaign = %campaign_id,
            "the tolerance index is empty: every already-characterized divergence will be \
             reported as new"
        );
    }

    let mut queue = AdmissionQueue::new(
        &existing.findings,
        &campaign_id,
        request.budget.admission_cap,
    );
    let mut counters = Counters::new();
    let mut observed: BTreeSet<String> = BTreeSet::new();
    let mut statuses: Vec<EntryStatus> = Vec::new();

    // An external temp root, reclaimed on both success and unwind. Corpus content is never
    // vendored (FR-053) and never lands in the repository: materializing under the
    // workspace would make `git status` the review surface for third-party bytes nobody
    // reviewed.
    let fetch_root = tempfile::Builder::new()
        .prefix("deacon-discovery-corpus-")
        .tempdir()
        .map_err(|e| HarnessError::Report {
            cause: format!("could not create the corpus fetch root: {e}"),
        })?;

    let wall_clock = Duration::from_secs(request.budget.wall_clock_seconds);
    let per_candidate = Duration::from_secs(request.budget.per_candidate_seconds);
    let fetch_bound = per_candidate.max(corpus_fetch::DEFAULT_ENTRY_BOUND);
    let started = Instant::now();

    for entry in &existing.corpus {
        if started.elapsed() >= wall_clock {
            break;
        }
        counters.generated += 1;

        let status = corpus_fetch::materialize(entry, fetch_root.path(), fetch_bound).await?;
        let materialized = match &status {
            EntryStatus::Materialized(m) => m.clone(),
            EntryStatus::Unreachable { cause, .. } => {
                counters.discarded_unsafe += 1;
                tracing::warn!(
                    entry = %entry.id,
                    name = %entry.name,
                    %cause,
                    "corpus entry is UNREACHABLE — nothing was compared for it"
                );
                statuses.push(status);
                continue;
            }
            EntryStatus::DigestMismatch {
                expected, actual, ..
            } => {
                counters.discarded_unsafe += 1;
                tracing::warn!(
                    entry = %entry.id,
                    name = %entry.name,
                    %expected,
                    %actual,
                    "corpus entry DIGEST MISMATCH — the pinned snapshot is not what was \
                     recorded, so it is not compared"
                );
                statuses.push(status);
                continue;
            }
        };

        let result = differential::compare(
            DifferentialInput {
                candidate_id: &entry.id,
                workspace: &materialized.workspace,
                deacon: &request.deacon_binary,
                oracle,
                bound: per_candidate,
                report_root: &request.report_root,
                // A real-world workspace is not deliberately malformed. deacon refusing
                // one is precisely the news this tier exists to surface, so it must never
                // be swallowed by the strictness characterization that covers deliberately
                // invalid candidates (FR-017 read narrowly, on purpose).
                deliberately_invalid: false,
            },
            &characterization,
        )
        .await;

        let result = match result {
            Ok(r) => r,
            Err(HarnessError::OracleTimeout { .. }) => {
                counters.timed_out += 1;
                tracing::debug!(
                    entry = %entry.id,
                    "corpus entry exceeded its per-candidate bound"
                );
                statuses.push(status);
                continue;
            }
            Err(other) => return Err(other),
        };

        counters.executed += 1;
        if result.parse_stage_failure {
            counters.parse_stage_failures += 1;
        }
        counters.characterized += result.characterized_count() as u64;
        for observation in &result.observations {
            observed.insert(observation.signature.id.clone());
        }

        // The witness's input NAMES the upstream provenance rather than embedding the
        // workspace (FR-054, FR-053): the content is not vendored, so the reproducible
        // thing is the pin plus the digest that says which bytes were compared.
        let provenance = serde_json::json!({
            "corpusEntry": entry.id,
            "name": entry.name,
            "repository": entry.repository,
            "commit": entry.commit,
            "path": entry.path,
            "contentDigest": materialized.digest,
        });
        for observation in result.new_observations() {
            let witness = Witness {
                id: Witness::derived_id(&campaign_id, &entry.id),
                campaign_id: campaign_id.clone(),
                // The corpus entry id IS the candidate id for this tier: the input was not
                // drawn from a stream, it is a pinned third-party snapshot, and naming it
                // anything else would lose the one thing that reproduces the observation.
                candidate_id: entry.id.clone(),
                minimal_input: provenance.clone(),
                // No reduction is performed, so the input is NOT minimal and says so
                // (FR-022). A real-world workspace is the least minimal input this feature
                // has; claiming otherwise would be the exact overstatement FR-022 forbids.
                is_minimal: false,
                reduction_steps: Vec::new(),
                observed_values: ObservedValues {
                    deacon: observation.observed.deacon.clone(),
                    reference: observation.observed.reference.clone(),
                },
                // No mutation operator produced this input; a third party did.
                mutation_operators: Vec::new(),
            };
            queue.offer(observation.signature.clone(), witness);
        }

        statuses.push(status);
    }

    // Record the digests this run settled, but only when the campaign is persisting at all
    // — an acceptance test running against an isolated root must not rewrite the committed
    // manifest, for the same reason it must not append to the committed queue.
    if request.persist {
        let written =
            corpus_fetch::record_digests(&request.discovery_dir, &existing.corpus, &statuses)?;
        if !written.is_empty() {
            tracing::info!(
                campaign = %campaign_id,
                entries = ?written,
                "recorded a content digest at first materialization; every later fetch \
                 verifies it"
            );
        }
    }

    let planned = existing.corpus.len() as u64;
    let mut run = complete(
        request,
        &existing.campaigns,
        Completion {
            campaign_id,
            pinned_input_set,
            counters,
            observed,
            queue,
            // The plan is the manifest — see this function's docs.
            planned,
            // No reviewable candidate is packaged here (T103's own note: this needs a
            // deliberate follow-up, not an assumption). The differential/metamorphic tiers'
            // `candidate::write` and `minimize::DifferentialProbe` were both built around a
            // candidate materialized from a single generated JSON document
            // (`campaign::materialize`), which can freely reduce and re-synthesize because
            // it owns the whole workspace tree it wrote. A corpus entry's workspace is a
            // real third-party directory `corpus_fetch::materialize` fetched, which may
            // contain a Dockerfile, a Compose file, local features, or other files the
            // devcontainer.json references by relative path — content this tier does not
            // own and must not vendor (FR-053). Reducing just the JSON portion through that
            // machinery could silently break those references and misreport "no longer
            // parses" as a reduction step rather than a broken reference. The witness
            // already names the upstream provenance (repository/commit/path/digest), which
            // is what FR-054 requires; packaging a six-part reviewable candidate for a
            // real-world workspace is a separate design question this tier defers rather
            // than answers by reusing machinery built for a different input shape.
            candidates: Vec::new(),
        },
    )?;
    run.corpus_statuses = statuses;
    Ok(run)
}

/// The seven pinned inputs (FR-002), built identically for every tier.
///
/// `oracle` is `None` for the metamorphic tier (and for the FR-042a pipeline proof), which
/// never invokes the reference — but the pin it *would* have been compared against is still
/// part of what makes its findings checkable, and the pinned input set has no optional
/// elements. One function rather than one per driver, so a tier cannot quietly record a
/// different set of pins than another.
pub(crate) fn pinned_input_set(
    grammar: &Grammar,
    oracle: Option<&VerifiedOracle>,
) -> Result<PinnedInputSet, HarnessError> {
    Ok(PinnedInputSet {
        schema_pin: deacon_conformance::CURRENT_SCHEMA_PIN.to_string(),
        prose_pin: deacon_conformance::CURRENT_SPEC_PIN.to_string(),
        oracle_version: match oracle {
            Some(o) => o.version.clone(),
            None => OraclePin::load()?.version,
        },
        normalizer_version: NORMALIZER_VERSION.to_string(),
        grammar_version: grammar.revision().to_string(),
        mutation_catalog_version: MUTATION_CATALOG_VERSION.to_string(),
        generator_version: deacon_conformance::discovery::generate::generator_identity(),
    })
}

/// What a finished driver hands to [`complete`].
struct Completion {
    campaign_id: String,
    pinned_input_set: PinnedInputSet,
    counters: Counters,
    observed: BTreeSet<String>,
    queue: AdmissionQueue,
    /// The denominator of `spaceCoveredFraction` — what the run *planned* to reach.
    planned: u64,
    /// The findings a reviewable candidate was emitted for. Empty for the metamorphic
    /// tier, which compares deacon against deacon and so has no two-sided evidence set to
    /// package.
    candidates: Vec<String>,
}

/// Turn a finished run into its record, persist it if asked, and build its report.
///
/// Shared by both drivers so the exhaustion semantics, the append-only campaign history,
/// and the report shape are one implementation. A second copy would be the one that starts
/// presenting a truncated run as complete.
fn complete(
    request: &CampaignRequest,
    existing_campaigns: &[Campaign],
    completion: Completion,
) -> Result<CampaignRun, HarnessError> {
    let Completion {
        campaign_id,
        pinned_input_set,
        mut counters,
        observed,
        queue,
        planned,
        candidates,
    } = completion;

    // Exhaustion is "we stopped short of the plan", whatever stopped us. Reporting it only
    // for the clock would let a run that ended early for any other reason present itself
    // as complete — the presentation FR-005 forbids.
    counters.budget_exhausted = counters.generated < planned;

    let space_covered_fraction = if planned == 0 {
        0.0
    } else {
        (counters.generated as f64 / planned as f64).clamp(0.0, 1.0)
    };

    let campaign = Campaign {
        id: campaign_id,
        seed: request.seed_hex.clone(),
        lane: request.lane,
        tier: request.tier,
        profile: request.profile.clone(),
        pinned_input_set,
        budget: request.budget,
        outcome: CampaignOutcome {
            candidates_generated: counters.generated,
            candidates_executed: counters.executed,
            candidates_discarded_unsafe: counters.discarded_unsafe + counters.timed_out,
            parse_stage_failures: counters.parse_stage_failures,
            budget_exhausted: counters.budget_exhausted,
            space_covered_fraction,
            mutation_applications: counters.mutations.clone(),
            signatures_observed: observed.len() as u64,
            signatures_admitted: queue.admitted.len() as u64,
            signatures_suppressed: queue.suppressed.len() as u64,
        },
    };

    reject_underived_witness(&queue.findings)?;

    if request.persist {
        let mut campaigns = existing_campaigns.to_vec();
        // Append-only: a campaign record is never rewritten, because a finding names the
        // campaign that observed it and a rewritten campaign would retroactively change
        // what that finding claims.
        if !campaigns.iter().any(|c| c.id == campaign.id) {
            campaigns.push(campaign.clone());
        }
        write_campaigns(&request.discovery_dir, &campaigns).map_err(|e| HarnessError::Report {
            cause: format!("could not write the campaign history: {e}"),
        })?;
        write_findings(&request.discovery_dir, &queue.findings).map_err(|e| {
            HarnessError::Report {
                cause: format!("could not write the findings queue: {e}"),
            }
        })?;
    }

    let report = build_campaign_outcome_report(&campaign, &queue.admitted);
    Ok(CampaignRun {
        campaign,
        findings: queue.findings,
        admitted: queue.admitted,
        report,
        characterized_observations: counters.characterized,
        shrink_probes: counters.shrink_probes,
        candidates_root: request.report_root.join("candidates"),
        candidates,
        // Filled in by the corpus driver after this returns; every other tier has none.
        corpus_statuses: Vec::new(),
    })
}

/// Refuse to hand back a queue whose witness ids the discovery loader would reject.
///
/// A witness id is substance-anchored over `campaignId ‖ candidateId`, and **D1** recomputes
/// it from those two stored fields. Each of the four witness-construction sites in this
/// crate derives its own id, and one of them hashed the drifted SIGNATURE while storing the
/// candidate — so the first real config-differential campaign wrote 14 records that
/// `discovery check` refused, and the queue could not be committed at all. A convention four
/// call sites must independently honour is one call site away from breaking again; this
/// makes it structural.
///
/// Fail-loud rather than repair-in-place: a mismatch means the writer and the identity rule
/// disagree about what a witness IS, and quietly rewriting the id would bury that.
fn reject_underived_witness(findings: &[Finding]) -> Result<(), HarnessError> {
    for finding in findings {
        for witness in &finding.witnesses {
            let derived = Witness::derived_id(&witness.campaign_id, &witness.candidate_id);
            if witness.id != derived {
                return Err(HarnessError::Report {
                    cause: format!(
                        "refusing to write a findings queue the loader would reject: witness \
                         `{}` on finding `{}` has an id that is not derived from its own \
                         `campaignId ‖ candidateId` (expected `{derived}`). A witness id is \
                         substance-anchored; whichever construction site produced this one is \
                         hashing something other than the two fields it stores.",
                        witness.id, finding.id
                    ),
                });
            }
        }
    }
    Ok(())
}

/// The findings queue plus one campaign's admission bookkeeping (FR-030/FR-034/FR-034b).
///
/// Shared by both drivers rather than reimplemented per tier. The deduplication rule (a
/// signature already in the standing queue is a re-witness, not an admission), the cap, and
/// the "suppression is counted, never silent" rule are one behavior; the copy that drifts is
/// the one that starts truncating quietly.
struct AdmissionQueue {
    /// The queue after this campaign's upserts.
    findings: Vec<Finding>,
    /// The finding ids present before the campaign started, so deduplication spans
    /// campaigns (FR-030/FR-034).
    known_before: BTreeSet<String>,
    /// The ids this campaign admitted or re-witnessed, in admission order.
    admitted: Vec<String>,
    /// The ids the cap turned away (FR-034b).
    suppressed: BTreeSet<String>,
    /// Signatures THIS campaign admitted that the standing queue did NOT already carry —
    /// what the cap is measured against (FR-034b). Re-witnessing a finding the queue
    /// already knows about must not consume this budget: `DEFAULT_ADMISSION_CAP`'s own
    /// docs describe it as a per-campaign limit on *newly distinct* signatures, and a
    /// queue that has grown past the cap would otherwise reach it on re-witnesses alone
    /// and permanently suppress every genuinely new signature from then on.
    newly_admitted: BTreeSet<String>,
    admission_cap: u64,
    campaign_id: String,
}

impl AdmissionQueue {
    fn new(existing: &[Finding], campaign_id: &str, admission_cap: u64) -> AdmissionQueue {
        AdmissionQueue {
            findings: existing.to_vec(),
            known_before: existing.iter().map(|f| f.id.clone()).collect(),
            admitted: Vec::new(),
            suppressed: BTreeSet::new(),
            newly_admitted: BTreeSet::new(),
            admission_cap,
            campaign_id: campaign_id.to_string(),
        }
    }

    /// Offer one observation to the queue.
    ///
    /// Two rules, both preserved verbatim by the extraction:
    ///
    /// - **A signature the standing queue already knows is never suppressed**, whatever the
    ///   cap. Refusing to re-witness something a reviewer has already seen would let the cap
    ///   quietly stop `lastObserved` from advancing, and a finding that stopped being
    ///   re-witnessed for that reason is indistinguishable from one that stopped
    ///   reproducing.
    /// - **Only a genuinely new signature can be suppressed**, and every suppression is
    ///   recorded (FR-034b) — never a silent truncation, so a campaign that keeps hitting
    ///   the cap is itself a visible signal that something systemic is diverging.
    ///
    /// The cap is measured against `newly_admitted`, not `admitted`: `admitted` holds every
    /// id this campaign *touched*, re-witnesses included, and measuring the cap against
    /// that would mean a campaign against a standing queue larger than the cap could never
    /// admit anything new — every campaign would exhaust its budget on re-witnesses before
    /// reaching its first genuinely new signature.
    /// Whether this campaign has never seen `finding_id` — neither in the standing queue
    /// nor earlier in its own run.
    ///
    /// A read-only query, added for T052 rather than a change to how admission works: it is
    /// what lets the driver decide whether minimization is worth an oracle invocation. A
    /// signature the queue already carries has a reduced input recorded against it, and
    /// re-deriving that costs a full reduction to reach the same document.
    fn is_new(&self, finding_id: &str) -> bool {
        !self.known_before.contains(finding_id) && !self.admitted.iter().any(|id| id == finding_id)
    }

    /// Whether the queue actually holds a record for `finding_id` after everything offered
    /// so far — false for a signature the cap turned away.
    fn holds(&self, finding_id: &str) -> bool {
        self.findings.iter().any(|f| f.id == finding_id)
    }

    fn offer(&mut self, signature: Signature, witness: Witness) {
        let finding_id = signature.finding_id();
        let already_known =
            self.known_before.contains(&finding_id) || self.admitted.contains(&finding_id);
        if !already_known && self.newly_admitted.len() as u64 >= self.admission_cap {
            self.suppressed.insert(finding_id);
            return;
        }
        upsert_finding(&mut self.findings, signature, witness, &self.campaign_id);
        if !self.admitted.contains(&finding_id) {
            self.admitted.push(finding_id.clone());
        }
        if !already_known {
            self.newly_admitted.insert(finding_id);
        }
    }
}

/// Resolve and verify the oracle, mapping every failure onto
/// [`HarnessError::OracleUnverified`] with the underlying cause preserved.
///
/// One variant rather than five so a caller can say "the reference could not be verified"
/// in a single match, while the message still names *which* prerequisite failed — the
/// distinction FR-003 draws between failing loudly and failing usefully.
async fn verified_oracle(request: &CampaignRequest) -> Result<VerifiedOracle, HarnessError> {
    let result = match &request.oracle_override {
        Some(path) => {
            let pin = OraclePin::load()?;
            crate::oracle::verify_binary(path, &pin, crate::oracle::VERSION_QUERY_BOUND).await
        }
        None => Oracle::acquire().await,
    };
    result.map_err(|e| HarnessError::OracleUnverified {
        cause: e.to_string(),
    })
}

/// A materialized candidate workspace, reclaimed when dropped.
///
/// The `.devcontainer/devcontainer.json` carries the candidate. The Compose file and
/// Dockerfile beside it are **fixture scaffolding**, not candidate content: a candidate
/// that names `docker-compose.yml` must find one, or the comparison would measure whether
/// each implementation reports a missing file the same way rather than how each resolves
/// the configuration. They are written at both the workspace root and inside
/// `.devcontainer/` because a Compose path is resolved against different anchors by
/// different code paths, and a generated candidate is not the place to adjudicate that.
pub(crate) struct CandidateWorkspace {
    dir: tempfile::TempDir,
}

impl CandidateWorkspace {
    pub(crate) fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// The minimal Compose project a Compose-shaped candidate needs to exist.
pub(crate) const COMPOSE_SCAFFOLD: &str = "services:\n  app:\n    image: alpine:3.19\n  db:\n    image: alpine:3.19\n  cache:\n    image: alpine:3.19\n";

/// The minimal Dockerfile a Dockerfile-shaped candidate needs to exist.
pub(crate) const DOCKERFILE_SCAFFOLD: &str = "FROM alpine:3.19\n";

fn materialize(candidate: &Candidate) -> std::io::Result<CandidateWorkspace> {
    materialize_document(&candidate.document)
}

/// Materialize an arbitrary configuration document into a throwaway workspace.
///
/// Shared with [`super::minimize`] and [`super::candidate`] rather than reimplemented per
/// caller. That is load-bearing rather than tidy: a minimization probe runs a *reduced*
/// document and asks whether the same signature still appears, so a probe workspace whose
/// scaffolding differed from the candidate workspace's would be measuring the scaffold. The
/// emitted candidate's `fixture/` tree is the same shape for the same reason — FR-027's
/// self-containment claim is that the fixture reproduces the observation, and it can only
/// do that if it is the tree the observation was made in.
pub(crate) fn materialize_document(document: &Value) -> std::io::Result<CandidateWorkspace> {
    let dir = tempfile::Builder::new()
        .prefix("deacon-discovery-")
        .tempdir()?;
    write_workspace_tree(dir.path(), document)?;
    Ok(CandidateWorkspace { dir })
}

/// Write the candidate workspace tree (config + scaffolding) into `root`.
pub(crate) fn write_workspace_tree(root: &Path, document: &Value) -> std::io::Result<()> {
    let config_dir = root.join(".devcontainer");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(
        config_dir.join("devcontainer.json"),
        serde_json::to_string_pretty(document)
            .unwrap_or_else(|e| unreachable!("a candidate document always serializes: {e}")),
    )?;
    for base in [root, config_dir.as_path()] {
        std::fs::write(base.join("docker-compose.yml"), COMPOSE_SCAFFOLD)?;
        std::fs::write(base.join("docker-compose.override.yml"), COMPOSE_SCAFFOLD)?;
        std::fs::write(base.join("Dockerfile"), DOCKERFILE_SCAFFOLD)?;
    }
    Ok(())
}

/// The running tallies a campaign accumulates.
struct Counters {
    generated: u64,
    executed: u64,
    discarded_unsafe: u64,
    timed_out: u64,
    parse_stage_failures: u64,
    characterized: u64,
    /// Probes minimization spent — reported so the cost of reduction is visible rather
    /// than folded invisibly into the wall clock.
    shrink_probes: u64,
    budget_exhausted: bool,
    mutations: ApplicationCounts,
}

impl Counters {
    fn new() -> Counters {
        Counters {
            generated: 0,
            executed: 0,
            discarded_unsafe: 0,
            timed_out: 0,
            parse_stage_failures: 0,
            characterized: 0,
            shrink_probes: 0,
            budget_exhausted: false,
            // All eleven keys from the start (FR-010): a category absent from the map is
            // indistinguishable from one that was never applied, so the map is never built
            // by inserting only the categories that fired.
            mutations: mutate::empty_application_counts(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_conformance::discovery::generate::{CandidateKind, Operation};
    use serde_json::json;

    fn candidate(id: &str, document: serde_json::Value) -> Candidate {
        Candidate {
            id: id.to_string(),
            index: 0,
            kind: CandidateKind::Valid,
            branch: None,
            fixture: None,
            document,
            mutations: Vec::new(),
            violated_required: Vec::new(),
            operations: vec![Operation::read_configuration()],
        }
    }

    #[test]
    fn a_materialized_workspace_carries_the_candidate_and_its_scaffolding() {
        let candidate = candidate(
            "cnd-11111111",
            json!({ "image": "alpine:3.19", "name": "m" }),
        );
        let workspace = materialize(&candidate).expect("materializes");
        let config = workspace
            .path()
            .join(".devcontainer")
            .join("devcontainer.json");
        let raw = std::fs::read_to_string(&config).expect("read back");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&raw).expect("valid JSON"),
            candidate.document,
            "the document written must be the document compared"
        );
        // Scaffolding at both anchors: a Compose-shaped candidate must find its file
        // whichever anchor an implementation resolves against, or the comparison would
        // measure missing-file reporting rather than configuration resolution.
        for base in [
            workspace.path().to_path_buf(),
            workspace.path().join(".devcontainer"),
        ] {
            assert!(base.join("docker-compose.yml").is_file());
            assert!(base.join("docker-compose.override.yml").is_file());
            assert!(base.join("Dockerfile").is_file());
        }
    }

    #[test]
    fn the_workspace_is_reclaimed_on_drop() {
        let candidate = candidate("cnd-22222222", json!({ "image": "alpine:3.19" }));
        let path = {
            let workspace = materialize(&candidate).expect("materializes");
            workspace.path().to_path_buf()
        };
        assert!(
            !path.exists(),
            "a campaign generates thousands of workspaces; one that outlives its candidate \
             fills the disk"
        );
    }

    // -----------------------------------------------------------------------
    // The shared admission queue (T096 extraction)
    // -----------------------------------------------------------------------

    fn signature(path: &str) -> Signature {
        Signature::derive(
            METAMORPHIC_CHANNEL,
            &deacon_conformance::discovery::signature::Divergence {
                kind: deacon_conformance::discovery::signature::DivergenceKind::Value,
                path,
                deacon: None,
                reference: None,
            },
        )
    }

    fn witness(campaign_id: &str, candidate_id: &str) -> Witness {
        Witness {
            id: Witness::derived_id(campaign_id, candidate_id),
            campaign_id: campaign_id.to_string(),
            candidate_id: candidate_id.to_string(),
            minimal_input: json!({}),
            is_minimal: false,
            reduction_steps: Vec::new(),
            observed_values: ObservedValues::default(),
            mutation_operators: Vec::new(),
        }
    }

    #[test]
    fn a_new_signature_is_admitted_once_however_often_it_is_offered() {
        let mut queue = AdmissionQueue::new(&[], "cmp-11111111", 25);
        let sig = signature("structuredOutput.configuration.name");

        queue.offer(sig.clone(), witness("cmp-11111111", "cnd-a"));
        queue.offer(sig.clone(), witness("cmp-11111111", "cnd-b"));

        assert_eq!(
            queue.admitted,
            vec![sig.finding_id()],
            "one signature is one finding however many witnesses it collects"
        );
        assert_eq!(queue.findings.len(), 1);
        assert_eq!(queue.findings[0].witnesses.len(), 2);
        assert!(queue.suppressed.is_empty());
    }

    /// The regression this guard exists for. The drift path used to hash the drifted
    /// SIGNATURE into the witness id while storing the candidate id beside it, so every
    /// drift witness failed **D1** the moment `discovery check` looked at it — and a
    /// campaign that shrank anything produced a queue nobody could commit.
    #[test]
    fn a_witness_id_hashed_from_anything_but_its_own_fields_is_refused_before_writing() {
        let mut queue = AdmissionQueue::new(&[], "cmp-11111111", 25);
        queue.offer(
            signature("structuredOutput.configuration.name"),
            witness("cmp-11111111", "cnd-a"),
        );
        assert!(
            reject_underived_witness(&queue.findings).is_ok(),
            "a queue whose witnesses derive their own ids must pass"
        );

        // Exactly the old drift-path mistake: a real campaign id, a real candidate id, and
        // an id hashed over something else entirely.
        queue.findings[0].witnesses[0].id = Witness::derived_id("cmp-11111111", "sig-deadbeef");
        let err = reject_underived_witness(&queue.findings)
            .expect_err("an id that is not derived from the stored fields must be refused");
        let message = err.to_string();
        assert!(
            message.contains(&Witness::derived_id("cmp-11111111", "cnd-a")),
            "the error must name the id the loader WILL derive, so the fix is mechanical: \
             {message}"
        );
    }

    #[test]
    fn the_cap_turns_away_new_signatures_and_reports_every_one_it_did() {
        let mut queue = AdmissionQueue::new(&[], "cmp-22222222", 2);
        let first = signature("structuredOutput.configuration.name");
        let second = signature("structuredOutput.configuration.image");
        let third = signature("structuredOutput.configuration.remoteUser");
        queue.offer(first.clone(), witness("cmp-22222222", "cnd-1"));
        queue.offer(second.clone(), witness("cmp-22222222", "cnd-2"));
        queue.offer(third.clone(), witness("cmp-22222222", "cnd-3"));

        assert_eq!(
            queue.admitted,
            vec![first.finding_id(), second.finding_id()],
            "the cap bounds admissions in offer order"
        );
        assert_eq!(
            queue.suppressed.iter().cloned().collect::<Vec<String>>(),
            vec![third.finding_id()],
            "the excess is REPORTED (FR-034b), never a silent truncation: a campaign that \
             keeps hitting the cap is itself a visible signal that something systemic is \
             diverging"
        );
        assert!(
            !queue.findings.iter().any(|f| f.id == third.finding_id()),
            "a suppressed signature must not reach the queue"
        );
    }

    #[test]
    fn a_signature_the_standing_queue_already_knows_is_never_suppressed() {
        let known = signature("structuredOutput.configuration.image");
        let mut seed = Vec::new();
        upsert_finding(
            &mut seed,
            known.clone(),
            witness("cmp-00000000", "cnd-old"),
            "cmp-00000000",
        );

        // A cap of ZERO, so nothing new can enter at all — and the standing finding still
        // collects its witness. Refusing to re-witness what a reviewer has already seen
        // would let the cap quietly stop `lastObserved` from advancing, and a finding that
        // stopped being re-witnessed for THAT reason is indistinguishable from one that
        // stopped reproducing.
        let mut queue = AdmissionQueue::new(&seed, "cmp-33333333", 0);
        queue.offer(known.clone(), witness("cmp-33333333", "cnd-new"));
        let fresh = signature("structuredOutput.configuration.name");
        queue.offer(fresh.clone(), witness("cmp-33333333", "cnd-other"));

        assert_eq!(queue.admitted, vec![known.finding_id()]);
        assert_eq!(
            queue.suppressed.iter().cloned().collect::<Vec<String>>(),
            vec![fresh.finding_id()],
            "a zero cap admits nothing NEW, which is exactly what it should mean"
        );
        let record = queue
            .findings
            .iter()
            .find(|f| f.id == known.finding_id())
            .expect("the standing finding survives");
        assert_eq!(record.witnesses.len(), 2, "it collected the new witness");
        assert_eq!(record.last_observed, "cmp-33333333");
    }

    #[test]
    fn the_counters_start_with_every_mutation_category_present() {
        let counters = Counters::new();
        assert_eq!(counters.mutations.len(), 11);
        assert!(counters.mutations.values().all(|&n| n == 0));
        assert_eq!(
            mutate::unapplied_categories(&counters.mutations).len(),
            11,
            "before anything runs, every category is unapplied — and says so"
        );
        assert!(!counters.budget_exhausted);
    }
}
