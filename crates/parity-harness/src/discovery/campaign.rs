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
//! The `metamorphic` tier (T096) and the `corpus` tier (T108) land with US6 and US7;
//! minimization (T052) with US2. This module currently drives the two differential tiers.
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
use deacon_conformance::load::Registry;

use crate::HarnessError;
use crate::normalize::NORMALIZER_VERSION;
use crate::oracle::{Oracle, OraclePin, VerifiedOracle};
use crate::prereq;

use super::differential::{self, Characterization, DifferentialInput};

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

    let pinned_input_set = PinnedInputSet {
        schema_pin: deacon_conformance::CURRENT_SCHEMA_PIN.to_string(),
        prose_pin: deacon_conformance::CURRENT_SPEC_PIN.to_string(),
        oracle_version: match oracle.as_ref() {
            Some(o) => o.version.clone(),
            // The metamorphic tier never invokes the reference, but the pin it would have
            // been compared against is still part of what makes its findings checkable —
            // and the pinned input set has no optional elements (FR-002).
            None => OraclePin::load()?.version,
        },
        normalizer_version: NORMALIZER_VERSION.to_string(),
        grammar_version: grammar.revision().to_string(),
        mutation_catalog_version: MUTATION_CATALOG_VERSION.to_string(),
        generator_version: deacon_conformance::discovery::generate::generator_identity(),
    };
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
    let mut findings = existing.findings.clone();
    let known_before: BTreeSet<String> = findings.iter().map(|f| f.id.clone()).collect();

    let mut generator = Generator::new(&grammar, request.seed);
    let mut counters = Counters::new();
    let mut admitted: Vec<String> = Vec::new();
    let mut suppressed: BTreeSet<String> = BTreeSet::new();
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
            // Only the metamorphic tier has no oracle, and its deacon-only evaluation does
            // not reach this loop (US6, T096). Arriving here would mean a tier was routed
            // into the differential without its prerequisite — which must fail rather than
            // compare against nothing.
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
            let finding_id = observation.signature.finding_id();
            let already_known =
                known_before.contains(&finding_id) || admitted.contains(&finding_id);
            if !already_known && admitted.len() as u64 >= request.budget.admission_cap {
                // FR-034b: never a silent truncation. The excess is reported, so a campaign
                // that keeps hitting the cap is itself a visible signal that something
                // systemic is diverging.
                suppressed.insert(finding_id);
                continue;
            }
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
            upsert_finding(
                &mut findings,
                observation.signature.clone(),
                witness,
                &campaign_id,
            );
            if !admitted.contains(&finding_id) {
                admitted.push(finding_id);
            }
        }

        drop(workspace);
    }

    // Exhaustion is "we stopped short of the plan", whatever stopped us. Reporting it only
    // for the clock would let a run that ended early for any other reason present itself
    // as complete — the presentation FR-005 forbids.
    counters.budget_exhausted = counters.generated < request.planned_candidates;

    let space_covered_fraction = if request.planned_candidates == 0 {
        0.0
    } else {
        (counters.generated as f64 / request.planned_candidates as f64).clamp(0.0, 1.0)
    };

    let campaign = Campaign {
        id: campaign_id.clone(),
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
            signatures_admitted: admitted.len() as u64,
            signatures_suppressed: suppressed.len() as u64,
        },
    };

    if request.persist {
        let mut campaigns = existing.campaigns.clone();
        // Append-only: a campaign record is never rewritten, because a finding names the
        // campaign that observed it and a rewritten campaign would retroactively change
        // what that finding claims.
        if !campaigns.iter().any(|c| c.id == campaign.id) {
            campaigns.push(campaign.clone());
        }
        write_campaigns(&request.discovery_dir, &campaigns).map_err(|e| HarnessError::Report {
            cause: format!("could not write the campaign history: {e}"),
        })?;
        write_findings(&request.discovery_dir, &findings).map_err(|e| HarnessError::Report {
            cause: format!("could not write the findings queue: {e}"),
        })?;
    }

    let report = build_campaign_outcome_report(&campaign, &admitted);
    Ok(CampaignRun {
        campaign,
        findings,
        admitted,
        report,
        characterized_observations: counters.characterized,
    })
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
