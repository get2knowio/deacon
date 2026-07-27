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
//! The `corpus` tier (T108) lands with US7 and minimization (T052) with US2. This module
//! drives the two differential tiers and the `metamorphic` tier (T096).
//!
//! ## Two shapes of campaign, one record
//!
//! [`run`] dispatches on the tier **before** acquiring any prerequisite, because the
//! metamorphic tier has none to acquire (research D12): it compares deacon against deacon
//! over a declared transformation, so there is no oracle to verify, no Docker to probe, and
//! no network to reach. Routing it through the differential's prerequisite step would make
//! the one tier a contributor can run with nothing installed depend on everything.
//!
//! Both shapes produce the **same** [`Campaign`] / [`CampaignOutcomeReport`] record and
//! admit through the same [`AdmissionQueue`], so a reader of `campaigns.json` does not need
//! to know which driver wrote a row, and the admission cap means the same thing on both.
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
    Candidate, Generator, unpinned_image_inputs, unsafe_reasons,
};
use deacon_conformance::discovery::grammar::Grammar;
use deacon_conformance::discovery::mutate::{self, ApplicationCounts, MUTATION_CATALOG_VERSION};
use deacon_conformance::discovery::queue::{
    Budget, Campaign, CampaignLane, CampaignOutcome, CampaignTier, DiscoveryData, Finding,
    ObservedValues, PinnedInputSet, Witness, upsert_finding, write_campaigns, write_findings,
};
use deacon_conformance::discovery::report::{CampaignOutcomeReport, build_campaign_outcome_report};
use deacon_conformance::discovery::signature::Signature;
use deacon_conformance::load::Registry;

use crate::HarnessError;
use crate::normalize::NORMALIZER_VERSION;
use crate::oracle::{Oracle, OraclePin, VerifiedOracle};
use crate::prereq;

use super::differential::{self, Characterization, DifferentialInput};
use super::metamorphic_run::{self, Sabotage};

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

        for observation in result.new_observations() {
            let witness = Witness {
                id: Witness::derived_id(&campaign_id, &candidate.id),
                campaign_id: campaign_id.clone(),
                candidate_id: candidate.id.clone(),
                minimal_input: candidate.document.clone(),
                // US1 performs no reduction, so the input is NOT minimal and says so.
                // FR-022 forbids presenting a partially reduced input as minimal; an
                // unreduced one even more so. Minimization lands with US2 (T047/T048/T052).
                is_minimal: false,
                reduction_steps: Vec::new(),
                observed_values: ObservedValues {
                    deacon: observation.observed.deacon.clone(),
                    reference: observation.observed.reference.clone(),
                },
                mutation_operators: candidate.operator_ids(),
            };
            queue.offer(observation.signature.clone(), witness);
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
        },
    )
}

/// The seven pinned inputs (FR-002), built identically for every tier.
///
/// `oracle` is `None` for the metamorphic tier, which never invokes the reference — but the
/// pin it *would* have been compared against is still part of what makes its findings
/// checkable, and the pinned input set has no optional elements. One function rather than
/// one per driver, so a tier cannot quietly record a different set of pins than another.
fn pinned_input_set(
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
    })
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
            self.admitted.push(finding_id);
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
struct CandidateWorkspace {
    dir: tempfile::TempDir,
}

impl CandidateWorkspace {
    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// The minimal Compose project a Compose-shaped candidate needs to exist.
const COMPOSE_SCAFFOLD: &str = "services:\n  app:\n    image: alpine:3.19\n  db:\n    image: alpine:3.19\n  cache:\n    image: alpine:3.19\n";

/// The minimal Dockerfile a Dockerfile-shaped candidate needs to exist.
const DOCKERFILE_SCAFFOLD: &str = "FROM alpine:3.19\n";

fn materialize(candidate: &Candidate) -> std::io::Result<CandidateWorkspace> {
    let dir = tempfile::Builder::new()
        .prefix("deacon-discovery-")
        .tempdir()?;
    let root = dir.path();
    let config_dir = root.join(".devcontainer");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(
        config_dir.join("devcontainer.json"),
        serde_json::to_string_pretty(&candidate.document)
            .unwrap_or_else(|e| unreachable!("a candidate document always serializes: {e}")),
    )?;
    for base in [root, config_dir.as_path()] {
        std::fs::write(base.join("docker-compose.yml"), COMPOSE_SCAFFOLD)?;
        std::fs::write(base.join("docker-compose.override.yml"), COMPOSE_SCAFFOLD)?;
        std::fs::write(base.join("Dockerfile"), DOCKERFILE_SCAFFOLD)?;
    }
    Ok(CandidateWorkspace { dir })
}

/// The running tallies a campaign accumulates.
struct Counters {
    generated: u64,
    executed: u64,
    discarded_unsafe: u64,
    timed_out: u64,
    parse_stage_failures: u64,
    characterized: u64,
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
