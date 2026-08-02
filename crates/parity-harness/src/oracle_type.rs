//! Oracle-type dispatch (research D8, T022, 022-conformance-runner).
//!
//! Dispatches on a case's [`OracleType`] to one of four distinct strategies:
//! **spec-expectation** (compare normalized observables to the declared `expected`, no
//! reference run), **snapshot** (compare to the committed provenance-checked snapshot),
//! **live-differential** (run deacon + the verified pinned oracle and compare), and
//! **invariant-metamorphic** (evaluate a declared relationship across ≥2 operations).
//! Re-pointing a case changes only `oracleType` (FR-007).
//!
//! MODULE STATUS: `spec-expectation` and `live-differential` are wired for User Story 1.
//! `snapshot` dispatch lands in US2 (T036) and the finalized four-way dispatch +
//! invariant/metamorphic evaluation in US6 (T068/T069). Until then those two types are
//! fail-loud (`HarnessError`), never a silent skip.

use crate::model::{ExpectedObservable, OracleType, TestCase};
use crate::snapshot;

use crate::HarnessError;
use crate::compare::{Tolerances, verdict_differential, verdict_spec_expectation};
use crate::evidence::{ChannelVerdict, NormalizedChannelEvidence, Outcome};
use crate::exec::Side;
use crate::runner::{self, RunConfig};

/// The result of evaluating a case's oracle: the per-channel verdicts plus the STALE
/// allowed differences (declared tolerances whose divergence did not reproduce this run,
/// FR-034) for the report.
pub type Evaluation = (Vec<ChannelVerdict>, Vec<String>);

/// Evaluate every declared channel of `case` under its `oracleType`, producing the
/// per-channel verdicts the runner aggregates into a [`CaseVerdict`](crate::evidence::CaseVerdict)
/// plus the stale allowed differences.
pub async fn evaluate(case: &TestCase, cfg: &RunConfig<'_>) -> Result<Evaluation, HarnessError> {
    match case.oracle_type {
        Some(OracleType::SpecExpectation) => spec_expectation(case, cfg).await,
        Some(OracleType::LiveDifferential) => live_differential(case, cfg).await,
        Some(OracleType::Snapshot) => snapshot_oracle(case, cfg).await,
        Some(OracleType::InvariantMetamorphic) => invariant_metamorphic(case, cfg).await,
        None => Err(HarnessError::NormalizationFailed {
            channel: format!("case:{}", case.id),
            cause: "declarative case has no `oracleType`".to_string(),
        }),
    }
}

/// spec-expectation: run deacon, capture + normalize each declared channel, evaluate the
/// declared `assertion` against it. Every `expected` MUST carry an assertion (validation
/// V16 guarantees it for a well-formed case; the runner still fails loud if one is
/// missing rather than silently pass).
async fn spec_expectation(
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<Evaluation, HarnessError> {
    let (ctx, ws) = runner::execute_ops(Side::Deacon, cfg.deacon_path, case, cfg).await?;
    // Capture every declared channel first, so the all-unobserved fault is diagnosed
    // BEFORE any assertion is evaluated against evidence that does not exist (D-2).
    let mut normalized = Vec::with_capacity(case.expected.len());
    for exp in &case.expected {
        normalized.push(runner::capture_normalized(case, exp, &ctx)?);
    }
    // Every observer has read what it needs; reclaim the case's Docker resources off the
    // executor before the (purely in-memory) assertion evaluation below.
    runner::release_workspace(ws).await?;
    require_some_observation(
        case,
        normalized.iter().map(|n| (n.channel.as_str(), n.present)),
    )?;

    let mut channels = Vec::with_capacity(case.expected.len());
    for (exp, normalized) in case.expected.iter().zip(&normalized) {
        let assertion =
            exp.assertion
                .as_ref()
                .ok_or_else(|| HarnessError::NormalizationFailed {
                    channel: exp.channel.clone(),
                    cause: format!(
                        "spec-expectation case {:?} declares channel {:?} without an assertion",
                        case.id, exp.channel
                    ),
                })?;
        let mut verdict = verdict_spec_expectation(&exp.channel, normalized, assertion)?;
        runner::attach_failure_phase(&mut verdict, case, exp, &ctx, None);
        channels.push(verdict);
    }
    // Allowed differences do not apply to spec-expectation (no reference to diverge from);
    // no stale computation here.
    Ok((channels, Vec::new()))
}

/// live-differential: run deacon AND the verified pinned oracle over the same
/// operations, capture + normalize each declared channel on both sides, and compare
/// them. A missing oracle is fail-loud (never a silent skip).
///
/// An `assertion`, when the case declares one, is evaluated against DEACON's side in
/// addition to the comparison — see the call site. The reference supplies the expectation
/// for most cases, which is why an assertion is optional here and required for
/// spec-expectation; but a differential alone cannot fail when both sides are equally
/// wrong, and a case that knows what the output should BE can say so.
async fn live_differential(
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<Evaluation, HarnessError> {
    let oracle = cfg.oracle.ok_or_else(|| HarnessError::OracleMissing {
        hint: format!(
            "live-differential case {:?} requires the verified pinned oracle, but none was \
             supplied to the runner",
            case.id
        ),
    })?;

    // -- deacon's side, captured and then RELEASED before the reference side runs -----
    //
    // The two sides run SEQUENTIALLY, and deacon's container/network/volumes are reclaimed
    // as soon as its evidence is in hand. Holding them across the reference run doubles the
    // peak Docker footprint and makes any case that publishes a FIXED host port collide
    // with itself (024 Phase 3, D-3).
    //
    // INVARIANT that makes this reordering correct: every Docker channel observer reads
    // only the PRE-FETCHED `ctx.container_inspect` (the runner fetches it inside its op
    // loop), never a live container; the filesystem observers read the workspace tree,
    // which is why the capture below happens BEFORE cleanup. No observer re-probes docker,
    // so nothing captured after this point depends on deacon's resources still existing.
    let (deacon_ctx, deacon_ws) =
        runner::execute_ops(Side::Deacon, cfg.deacon_path, case, cfg).await?;
    let mut deacon_norm = Vec::with_capacity(case.expected.len());
    for exp in &case.expected {
        deacon_norm.push(runner::capture_normalized(case, exp, &deacon_ctx)?);
    }
    // Reclamation shells out to `deacon down` plus several `docker` calls, all via blocking
    // `std::process::Command::output`, so it is offloaded (see `release_workspace`).
    runner::release_workspace(deacon_ws).await?;

    // -- the reference side ----------------------------------------------------------
    let (oracle_ctx, oracle_ws) =
        runner::execute_ops(Side::Oracle, &oracle.path, case, cfg).await?;
    let mut oracle_norm = Vec::with_capacity(case.expected.len());
    for exp in &case.expected {
        oracle_norm.push(runner::capture_normalized(case, exp, &oracle_ctx)?);
    }
    // Symmetric with deacon's side: the reference's resources are reclaimed as soon as its
    // evidence is in hand, so nothing is held across the comparison below.
    runner::release_workspace(oracle_ws).await?;

    // A channel neither side observed is not evidence of agreement (D-2). Individual
    // channels keep FR-018's not-captured-matches-not-captured; only a case where NOTHING
    // was observed is rejected — it proved nothing.
    require_some_observation(
        case,
        deacon_norm
            .iter()
            .zip(&oracle_norm)
            .map(|(d, o)| (d.channel.as_str(), d.present || o.present)),
    )?;

    let tolerances = Tolerances::new(&case.allowed_differences, &case.behaviors);
    let mut consumed = std::collections::HashSet::new();
    let mut channels = Vec::with_capacity(case.expected.len());
    for ((exp, deacon_norm), oracle_norm) in
        case.expected.iter().zip(&deacon_norm).zip(&oracle_norm)
    {
        let mut verdict = verdict_differential(
            &exp.channel,
            deacon_norm,
            oracle_norm,
            &tolerances,
            &mut consumed,
        );
        // An ABSOLUTE claim, when the case declares one, on top of the relative one.
        //
        // A differential alone passes when both sides are equally wrong — two CLIs that
        // each fail to recover a value both print the empty string and agree perfectly.
        // For most cases that is the right and only available answer; but where the case
        // knows what the output should BE, saying so turns "they match" into "they match
        // AND it is correct". Until this ran, an `assertion` on a live-differential
        // expectation was silently dead data: authored, loaded, and never evaluated.
        //
        // Checked against deacon's side. The reference is the thing being compared to, not
        // a thing under test; a reference that disagreed with the declared expectation is
        // already reported by the differential above.
        if let Some(assertion) = &exp.assertion {
            let declared = verdict_spec_expectation(&exp.channel, deacon_norm, assertion)?;
            if declared.outcome != Outcome::Agree {
                verdict = declared;
            }
        }
        runner::attach_failure_phase(&mut verdict, case, exp, &deacon_ctx, Some(&oracle_ctx));
        channels.push(verdict);
    }
    Ok((channels, tolerances.stale(&consumed)))
}

/// Channels whose evidence comes from the RUNTIME (a container or the filesystem) rather
/// than from the CLI process itself.
///
/// The distinction is what makes the vacuity guard bite. A CLI-process channel
/// (`chan-exit-code`, `chan-stdout`, …) is captured from the invocation and is therefore
/// present on essentially every run, including a run where Docker fell over. A
/// runtime-derived channel is present only if the container or file tree was actually
/// inspected. Requiring "at least one channel overall" is satisfied by `chan-exit-code`
/// alone, which is why it must be required PER GROUP instead.
const RUNTIME_DERIVED_CHANNELS: &[&str] = &[
    crate::model::CHAN_CONTAINER_STATE,
    crate::model::CHAN_IMAGE,
    crate::model::CHAN_PROCESS_GRAPH,
    crate::model::CHAN_INJECTED_PROCESS,
    crate::model::CHAN_TEMPORAL,
    crate::model::CHAN_FILESYSTEM,
    crate::model::CHAN_FILE_CONTENT,
];

/// Fail loud when a case's declared channels were not actually observed (024 Phase 3,
/// D-2).
///
/// `channels` yields `(channel-id, observed)` for each declared channel — for a
/// differential, `observed` is true when EITHER side captured it. Two rules:
///
/// 1. **Nothing at all was observed.** A comparison of two absences is not agreement.
/// 2. **Runtime-derived channels were declared and NONE of them was observed.** This is
///    the rule that matters in practice. Rule 1 alone is nearly vacuous for a Docker case:
///    such a case also declares `chan-exit-code`, which is captured from the CLI process
///    and is present even when the daemon is dead — so a run where `docker inspect` came
///    back empty for every observer would satisfy rule 1 on the exit code alone, compare
///    not-captured to not-captured across all four Docker channels, and report `agree`.
///    That is exactly the silent green pass this guard exists to eliminate, surviving the
///    guard.
///
/// Individual channels keep the FR-018 semantics (not-captured matches not-captured) —
/// a case may legitimately observe 3 of its 4 Docker channels. What is rejected is a
/// whole GROUP coming back empty, because that indicates the observation itself failed
/// rather than the evidence being genuinely absent. A case declaring NO channel is left to
/// the other validators (V16).
fn require_some_observation<'a>(
    case: &TestCase,
    channels: impl IntoIterator<Item = (&'a str, bool)>,
) -> Result<(), HarnessError> {
    let mut any_observed = false;
    let mut all: Vec<&str> = Vec::new();
    let mut runtime: Vec<&str> = Vec::new();
    let mut any_runtime_observed = false;

    for (channel, observed) in channels {
        all.push(channel);
        any_observed |= observed;
        if RUNTIME_DERIVED_CHANNELS.contains(&channel) {
            runtime.push(channel);
            any_runtime_observed |= observed;
        }
    }

    if all.is_empty() {
        return Ok(());
    }
    if !any_observed {
        return Err(HarnessError::ObservationFault {
            case: case.id.clone(),
            cause: format!(
                "every declared channel came back not-captured ({}); a comparison of two \
                 absences is not agreement",
                all.join(", ")
            ),
        });
    }
    if !runtime.is_empty() && !any_runtime_observed {
        return Err(HarnessError::ObservationFault {
            case: case.id.clone(),
            cause: format!(
                "no runtime-derived channel was captured ({}) — only CLI-process channels \
                 were, which are present even when the container runtime is unavailable; \
                 comparing these absences would report agreement about a container nobody \
                 inspected",
                runtime.join(", ")
            ),
        });
    }
    Ok(())
}

/// snapshot: resolve the committed snapshot for the current `os-arch`, gate on
/// provenance staleness, then compare deacon's freshly-normalized evidence to the
/// snapshot's committed normalized evidence (T036, D5). Emits `no-reference-for-platform`
/// when no snapshot exists for the platform, `stale` (all channels) when a provenance
/// field drifted, else per-channel `agree`/`diverge` against the recorded evidence.
async fn snapshot_oracle(case: &TestCase, cfg: &RunConfig<'_>) -> Result<Evaluation, HarnessError> {
    let os_arch = snapshot::current_os_arch();
    let resolution = snapshot::resolve(cfg.snapshots_root, &os_arch, &case.id).map_err(|e| {
        HarnessError::NormalizationFailed {
            channel: format!("case:{}", case.id),
            cause: format!("could not load committed snapshot: {e}"),
        }
    })?;
    let snap = match resolution {
        snapshot::Resolution::NoReferenceForPlatform { os_arch } => {
            return Ok((
                all_channels(case, Outcome::NoReferenceForPlatform, || {
                    Some(serde_json::json!({ "osArch": os_arch }))
                }),
                Vec::new(),
            ));
        }
        snapshot::Resolution::Found(s) => *s,
    };

    // Staleness gate: recompute the evidence-determining inputs and compare to committed
    // provenance. Host tool versions (node/docker/compose) are informational, NOT staleness
    // signals (see `snapshot::compare_staleness`), so they are neither re-probed nor
    // compared — a snapshot recorded under a different Node/Docker/Compose must replay
    // fresh across machines (SC-003).
    let (case_hash, fixture_hash) = runner::snapshot_hashes(case, cfg)?;
    let mut current = snap.provenance.clone();
    current.case_hash = case_hash;
    current.fixture_hash = fixture_hash;
    current.source_revision = crate::CURRENT_SPEC_PIN.to_string();
    current.normalizer_version = snapshot::NORMALIZER_VERSION.to_string();
    if let Some(v) = snapshot::current_oracle_version_pin() {
        current.oracle_version = v;
    }
    // Recompute imageDigests for a Docker case (finding #5) so a changed pinned image is
    // caught; offloaded so the docker probe never blocks the async executor (finding #4).
    // `None` (docker unreachable) carries the recorded digests rather than fabricating.
    let case_for_digests = case.clone();
    let fixtures_root = cfg.fixtures_root.to_path_buf();
    let recomputed_digests = tokio::task::spawn_blocking(move || {
        runner::image_digests_for_case(&case_for_digests, &fixtures_root)
    })
    .await
    .map_err(runner::blocking_join_err)?;
    if let Some(digests) = recomputed_digests {
        current.image_digests = digests.into_iter().collect();
    }
    if let snapshot::Staleness::Stale { field, .. } =
        snapshot::compare_staleness(&snap.provenance, &current)
    {
        return Ok((
            all_channels(case, Outcome::Stale, || {
                Some(serde_json::json!({ "staleField": field }))
            }),
            Vec::new(),
        ));
    }

    // Fresh: run deacon and compare its normalized evidence to the recorded evidence.
    let recorded: Vec<NormalizedChannelEvidence> = serde_json::from_value(snap.normalized.clone())
        .map_err(|e| HarnessError::NormalizationFailed {
            channel: format!("case:{}", case.id),
            cause: format!("committed normalized.json is not channel evidence: {e}"),
        })?;
    let (ctx, ws) = runner::execute_ops(Side::Deacon, cfg.deacon_path, case, cfg).await?;
    let tolerances = Tolerances::new(&case.allowed_differences, &case.behaviors);
    let mut consumed = std::collections::HashSet::new();
    let mut channels = Vec::with_capacity(case.expected.len());
    let mut captured = Vec::with_capacity(case.expected.len());
    for exp in &case.expected {
        captured.push(runner::capture_normalized(case, exp, &ctx)?);
    }
    // Observation complete — reclaim before the in-memory comparison (see the note on
    // `release_workspace`).
    runner::release_workspace(ws).await?;
    for (exp, deacon_norm) in case.expected.iter().zip(&captured) {
        let op = runner::resolve_expected_op(case, exp)?;
        match recorded
            .iter()
            .find(|e| e.channel == exp.channel && e.operation == op.id)
        {
            Some(rec) => channels.push(verdict_differential(
                &exp.channel,
                deacon_norm,
                rec,
                &tolerances,
                &mut consumed,
            )),
            None => channels.push(ChannelVerdict {
                channel: exp.channel.clone(),
                outcome: Outcome::Diverge,
                detail: Some(
                    serde_json::json!({ "reason": "channel absent from committed snapshot" }),
                ),
            }),
        }
    }
    Ok((channels, tolerances.stale(&consumed)))
}

/// invariant-metamorphic: run the case's operations and verdict on the DECLARED
/// RELATIONSHIP between operations (idempotence / first-create-vs-restart / resume),
/// NOT against a fixed expected output (FR-008, T069). Each operation that declares a
/// `relationship` is evaluated against its sibling operation's container snapshot,
/// producing one `chan-temporal` verdict per relationship.
async fn invariant_metamorphic(
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<Evaluation, HarnessError> {
    let (ctx, ws) = runner::execute_ops(Side::Deacon, cfg.deacon_path, case, cfg).await?;

    let mut channels = Vec::new();
    for op in &case.operations {
        if let Some(rel) = &op.relationship {
            channels.push(evaluate_relationship(&op.id, rel, &ctx));
        }
    }
    // The relationships are evaluated from the already-captured per-op snapshots, so the
    // case's Docker resources are reclaimed here — off the executor.
    runner::release_workspace(ws).await?;
    if channels.is_empty() {
        // Validation (V20) guarantees a metamorphic case declares a relationship; the
        // runner still fails loud rather than silently pass an unverifiable case.
        return Err(HarnessError::NormalizationFailed {
            channel: format!("case:{}", case.id),
            cause: "invariant/metamorphic case declares no operation `relationship` to evaluate"
                .to_string(),
        });
    }
    Ok((channels, Vec::new()))
}

/// Evaluate one declared relationship between `op_id` and its sibling `relationship.againstOp`
/// on their container snapshots, producing a `chan-temporal` verdict. `agree` when the
/// relationship holds; `diverge` (with a path-free reason) when it is violated or the
/// state needed to evaluate it is missing.
fn evaluate_relationship(
    op_id: &str,
    relationship: &crate::model::Relationship,
    ctx: &crate::observe::RunContext,
) -> ChannelVerdict {
    use crate::model::{CHAN_TEMPORAL, RelationshipKind};

    let temporal = CHAN_TEMPORAL.to_string();
    let this = ctx.op_snapshot(op_id);
    let sibling = ctx.op_snapshot(&relationship.against_op);
    let (this, sibling) = match (this, sibling) {
        (Some(t), Some(s)) => (t, s),
        _ => {
            return ChannelVerdict {
                channel: temporal,
                outcome: Outcome::Diverge,
                detail: Some(serde_json::json!({
                    "kind": "metamorphic",
                    "relationship": format!("{:?}", relationship.kind),
                    "reason": "missing container snapshot for the operation or its sibling \
                               (a metamorphic case must be Docker-backed)",
                })),
            };
        }
    };

    // The core invariant across all three kinds: the sibling created a container, and this
    // operation REUSED THE SAME SINGLE container (deacon did not recreate it OR leave a
    // second one behind) and it is running. `this.container_ids` is the FULL set matching
    // the workspace at this op's boundary — a non-idempotent op that created a second
    // container has `len > 1`, which must NOT count as "reused"; checking a single id would
    // mask exactly the failure this oracle exists to catch (finding #3).
    let this_count = this.container_ids.len();
    let same_container =
        this_count == 1 && this.container_id.is_some() && this.container_id == sibling.container_id;
    let running = this.temporal.get("running") == Some(&serde_json::Value::Bool(true));

    let (holds, detail_reason) = match relationship.kind {
        // Re-running the op is a no-op on the container: same single container, still running.
        RelationshipKind::Idempotence => (
            same_container && running,
            "re-run reuses the same single running container (no new create)",
        ),
        // The first op created the container; this op restarted/reused it (not recreated).
        RelationshipKind::FirstCreateVsRestart => (
            sibling.container_id.is_some() && same_container && running,
            "the sibling created the container; this op reused it (restart, not recreate)",
        ),
        // This op reattached to the sibling's existing container.
        RelationshipKind::Resume => (
            same_container && running,
            "this op resumed the sibling's existing container",
        ),
    };

    ChannelVerdict {
        channel: temporal,
        outcome: if holds {
            Outcome::Agree
        } else {
            Outcome::Diverge
        },
        detail: Some(serde_json::json!({
            "kind": "metamorphic",
            "relationship": format!("{:?}", relationship.kind),
            "againstOp": relationship.against_op,
            "held": holds,
            "expectation": detail_reason,
            "sameContainer": same_container,
            "containerCount": this_count,
            "running": running,
        })),
    }
}

/// Build one [`ChannelVerdict`] per declared channel with a fixed `outcome` and a
/// per-channel `detail` (used for the case-level `no-reference-for-platform` / `stale`
/// outcomes, which apply to every channel).
fn all_channels(
    case: &TestCase,
    outcome: Outcome,
    detail: impl Fn() -> Option<serde_json::Value>,
) -> Vec<ChannelVerdict> {
    if case.expected.is_empty() {
        // No declared channel: still surface the case-level outcome on a synthetic row.
        return vec![ChannelVerdict {
            channel: "case".to_string(),
            outcome,
            detail: detail(),
        }];
    }
    case.expected
        .iter()
        .map(|exp: &ExpectedObservable| ChannelVerdict {
            channel: exp.channel.clone(),
            outcome,
            detail: detail(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OracleType;

    fn cfg<'a>(
        fixtures_root: &'a std::path::Path,
        snapshots_root: &'a std::path::Path,
    ) -> RunConfig<'a> {
        RunConfig {
            deacon_path: std::path::Path::new("/bin/true"),
            oracle: None,
            fixtures_root,
            report_root: std::path::Path::new("/tmp"),
            snapshots_root,
        }
    }

    #[tokio::test]
    async fn metamorphic_case_with_no_relationship_fails_loud() {
        // A metamorphic case that declares no operation `relationship` is unverifiable —
        // fail loud rather than silently pass (validation V20 also rejects it).
        let case = TestCase {
            id: "case-x".to_string(),
            oracle_type: Some(OracleType::InvariantMetamorphic),
            operations: vec![crate::model::Operation {
                id: "op".to_string(),
                subcommand: "read-configuration".to_string(),
                ..Default::default()
            }],
            ..TestCase::default()
        };
        let tmp = std::path::Path::new("/tmp");
        assert!(evaluate(&case, &cfg(tmp, tmp)).await.is_err());
    }

    #[test]
    fn evaluate_relationship_verdicts_on_the_declared_relationship() {
        use crate::model::{Relationship, RelationshipKind};
        use crate::observe::{OpSnapshot, RunContext};

        let running =
            serde_json::json!({ "status": "running", "running": true, "restartCount": 0 });
        let mut ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
        // op-up-1 created container "c1"; op-up-2 REUSED "c1" (idempotent re-up).
        ctx.record_op_snapshot(
            "op-up-1",
            OpSnapshot {
                container_id: Some("c1".to_string()),
                container_ids: vec!["c1".to_string()],
                temporal: running.clone(),
            },
        );
        ctx.record_op_snapshot(
            "op-up-2",
            OpSnapshot {
                container_id: Some("c1".to_string()),
                container_ids: vec!["c1".to_string()],
                temporal: running.clone(),
            },
        );
        let rel = Relationship {
            kind: RelationshipKind::Idempotence,
            against_op: "op-up-1".to_string(),
        };
        // The relationship HOLDS: same single running container → agree (NOT a fixed value).
        let held = evaluate_relationship("op-up-2", &rel, &ctx);
        assert_eq!(held.outcome, Outcome::Agree, "idempotence holds: {held:?}");
        assert_eq!(held.channel, crate::model::CHAN_TEMPORAL);

        // If op-up-2 had RECREATED the container (different id), idempotence is VIOLATED.
        let mut ctx2 = RunContext::new(std::path::PathBuf::from("/tmp"));
        ctx2.record_op_snapshot(
            "op-up-1",
            OpSnapshot {
                container_id: Some("c1".to_string()),
                container_ids: vec!["c1".to_string()],
                temporal: running.clone(),
            },
        );
        ctx2.record_op_snapshot(
            "op-up-2",
            OpSnapshot {
                container_id: Some("c2-recreated".to_string()),
                container_ids: vec!["c2-recreated".to_string()],
                temporal: running.clone(),
            },
        );
        let violated = evaluate_relationship("op-up-2", &rel, &ctx2);
        assert_eq!(
            violated.outcome,
            Outcome::Diverge,
            "a recreated container violates idempotence: {violated:?}"
        );

        // If op-up-2 LEFT A SECOND container behind (old "c1" kept + new "c2" created), the
        // set matching the workspace is {c1, c2}. The primary id still equals the sibling's
        // "c1", so an id-only check would FALSELY pass — the count guard (finding #3) catches
        // it: two containers is not "reused the same single container".
        let mut ctx3 = RunContext::new(std::path::PathBuf::from("/tmp"));
        ctx3.record_op_snapshot(
            "op-up-1",
            OpSnapshot {
                container_id: Some("c1".to_string()),
                container_ids: vec!["c1".to_string()],
                temporal: running.clone(),
            },
        );
        ctx3.record_op_snapshot(
            "op-up-2",
            OpSnapshot {
                container_id: Some("c1".to_string()),
                container_ids: vec!["c1".to_string(), "c2".to_string()],
                temporal: running,
            },
        );
        let two_containers = evaluate_relationship("op-up-2", &rel, &ctx3);
        assert_eq!(
            two_containers.outcome,
            Outcome::Diverge,
            "a second leftover container violates idempotence even when the primary id \
             matches (finding #3): {two_containers:?}"
        );
    }

    #[tokio::test]
    async fn snapshot_missing_for_platform_yields_no_reference() {
        // No committed snapshot under the empty snapshots root → no-reference-for-platform.
        let dir = tempfile::tempdir().expect("tempdir");
        let case = TestCase {
            id: "case-missing-snap".to_string(),
            oracle_type: Some(OracleType::Snapshot),
            operations: vec![crate::model::Operation {
                id: "op".to_string(),
                subcommand: "read-configuration".to_string(),
                ..Default::default()
            }],
            expected: vec![crate::model::ExpectedObservable {
                channel: crate::model::CHAN_EXIT_CODE.to_string(),
                operation: Some("op".to_string()),
                assertion: None,
            }],
            ..TestCase::default()
        };
        let (channels, _stale) = evaluate(&case, &cfg(dir.path(), dir.path()))
            .await
            .expect("no-reference is a verdict, not an error");
        assert!(
            channels
                .iter()
                .all(|c| c.outcome == Outcome::NoReferenceForPlatform),
            "a missing snapshot for this os-arch is no-reference-for-platform, got {channels:?}"
        );
    }

    #[tokio::test]
    async fn live_differential_without_oracle_fails_loud() {
        let case = TestCase {
            id: "case-x".to_string(),
            oracle_type: Some(OracleType::LiveDifferential),
            operations: vec![crate::model::Operation {
                id: "op".to_string(),
                subcommand: "read-configuration".to_string(),
                ..Default::default()
            }],
            expected: vec![crate::model::ExpectedObservable {
                channel: crate::model::CHAN_EXIT_CODE.to_string(),
                operation: None,
                assertion: None,
            }],
            ..TestCase::default()
        };
        let tmp = std::path::Path::new("/tmp");
        assert!(matches!(
            evaluate(&case, &cfg(tmp, tmp)).await,
            Err(HarnessError::OracleMissing { .. })
        ));
    }
}
