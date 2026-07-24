//! The declarative conformance runner orchestration (T023, 022-conformance-runner).
//!
//! [`run_case`] loads a declarative [`TestCase`], runs its operations against the
//! target(s), invokes the declared observers, normalizes (the single
//! [`crate::normalize`]), compares per `oracleType` ([`crate::oracle_type`]), and emits
//! a [`CaseVerdict`]. Missing oracle / missing fixtures / unsupported channels are
//! fail-loud [`HarnessError`]s, never a silent skip (constitution IV).
//!
//! US1 wires the CLI-process channels for the `spec-expectation` and `live-differential`
//! oracle types. Config-only operations (`read-configuration`, `doctor`) run against the
//! committed fixture directory directly (read-only, no mutation); the isolated external
//! temp workspace + RAII cleanup for Docker-backed cases lands in US5 (`workspace.rs`).

use std::path::{Path, PathBuf};

use deacon_conformance::model::{
    CHAN_EXIT_CODE, CaseKind, ExpectedObservable, Operation, ResourceGroup, TestCase,
};

use crate::HarnessError;
use crate::evidence::{CaseVerdict, ChannelVerdict, NormalizedChannelEvidence, RawChannelEvidence};
use crate::exec::{ExecKind, Side, run_and_capture};
use crate::observe::{ProcessOutcome, RunContext, cli_process, observer_for};
use crate::oracle::VerifiedOracle;
use crate::workspace::DockerWorkspace;

/// The raw-capture binary key for the runner's invocations (the `raw/<binary>/…`
/// subtree under the report root).
pub const RUNNER_BINARY: &str = "conformance_runner";

/// The `${WORKSPACE}` token substituted in an operation's argv with the resolved
/// workspace path (contract case-schema.md).
const WORKSPACE_TOKEN: &str = "${WORKSPACE}";

/// Everything the runner needs from its caller: the deacon binary under test, the
/// verified oracle (required only for `live-differential`), where fixtures live, and
/// where to write raw capture. The binary paths are supplied explicitly — only the test
/// crate can expand `env!("CARGO_BIN_EXE_deacon")`, and the harness never guesses a
/// `target/…` path (mirrors [`crate::exec`]).
#[derive(Debug, Clone)]
pub struct RunConfig<'a> {
    /// Path to the deacon binary under test.
    pub deacon_path: &'a Path,
    /// The verified pinned oracle (required for `live-differential`; `None` otherwise).
    pub oracle: Option<&'a VerifiedOracle>,
    /// Root under which a fixture id resolves to `<fixtures_root>/<fixture-id>/`.
    pub fixtures_root: &'a Path,
    /// Root the raw stdout/stderr artifacts are written under (atomic temp+rename).
    pub report_root: &'a Path,
    /// Committed-snapshots root (`conformance/snapshots`) — the `snapshot` oracle type
    /// resolves `<snapshots_root>/<os-arch>/<case-id>/` here. Unused by
    /// spec-expectation / live-differential.
    pub snapshots_root: &'a Path,
}

/// Run one declarative case end to end and produce its [`CaseVerdict`].
pub async fn run_case(case: &TestCase, cfg: &RunConfig<'_>) -> Result<CaseVerdict, HarnessError> {
    // Only declarative cases run through the runner; a legacy/mixed/neither record is a
    // fail-loud authoring error (the loader/validator already reject it, but the runner
    // never silently accepts one either).
    match case.classify() {
        Ok(CaseKind::Declarative) => {}
        Ok(CaseKind::Legacy) => {
            return Err(shape_error(
                case,
                "legacy binary-backed case cannot be run by the declarative runner",
            ));
        }
        Err(shape) => return Err(shape_error(case, shape.message())),
    }
    let oracle_type = case
        .oracle_type
        .ok_or_else(|| shape_error(case, "declarative case has no `oracleType`"))?;

    let (channels, stale_allowed_differences) = crate::oracle_type::evaluate(case, cfg).await?;
    let overall = CaseVerdict::compute_overall(&channels);
    Ok(CaseVerdict {
        case_id: case.id.clone(),
        oracle_type,
        behaviors: case.behaviors.clone(),
        channels,
        overall,
        stale_allowed_differences,
    })
}

/// Run every operation of `case` against `program` on `side`, returning a
/// [`RunContext`] carrying each operation's [`ProcessOutcome`]. Shared by the
/// spec-expectation (deacon only) and live-differential (deacon + oracle) paths.
pub(crate) async fn execute_ops(
    side: Side,
    program: &Path,
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<(RunContext, Option<DockerWorkspace>), HarnessError> {
    // Docker-backed cases run in an ISOLATED external temp workspace (US5) so their
    // container identity + labels are unique (collision-safe) and an RAII guard reclaims
    // every resource on success AND unwind. Config-only cases run against the committed
    // fixture directory directly (read-only, no container).
    let docker_case = is_docker_case(case);
    let mut docker_ws: Option<DockerWorkspace> = None;
    let isolated_workspace: Option<PathBuf> = if docker_case {
        let ws = DockerWorkspace::new(Some(cfg.deacon_path)).map_err(|e| {
            HarnessError::DockerUnavailable {
                cause: format!("could not create an isolated workspace: {e}"),
            }
        })?;
        for id in unique_fixture_ids(case) {
            let dir = cfg.fixtures_root.join(&id);
            if !dir.is_dir() {
                return Err(HarnessError::FixtureMissing { path: dir });
            }
            ws.materialize(&dir)
                .map_err(|e| HarnessError::FixtureMissing {
                    path: dir.join(format!("<materialize failed: {e}>")),
                })?;
        }
        let path = ws.path().to_path_buf();
        docker_ws = Some(ws);
        Some(path)
    } else {
        None
    };

    let mut context_workspace: Option<PathBuf> = None;
    let mut outcomes: Vec<(String, ProcessOutcome)> = Vec::new();
    let mut op_snapshots: Vec<(String, crate::observe::OpSnapshot)> = Vec::new();
    let mut container_id: Option<String> = None;
    // The final `up` container's full `docker inspect`, captured ONCE (off the executor)
    // and handed to every Docker channel observer via `RunContext` (finding #4).
    let mut container_inspect: Option<serde_json::Value> = None;

    for op in &case.operations {
        // Docker cases: one isolated workspace for every op. Config-only: per-op fixture.
        let workspace = match &isolated_workspace {
            Some(ws) => ws.clone(),
            None => resolve_workspace(case, op, cfg)?,
        };
        if context_workspace.is_none() {
            context_workspace = Some(workspace.clone());
        }
        // For a Docker case every op shares the ISOLATED workspace (materialized once), so
        // `${WORKSPACE}` always resolves even for a later op that declares no fixture.
        let argv = substitute_argv(case, op, &workspace, isolated_workspace.is_some())?;
        let mut full: Vec<String> = Vec::with_capacity(argv.len() + 1);
        full.push(op.subcommand.clone());
        full.extend(argv);
        let args: Vec<&str> = full.iter().map(String::as_str).collect();

        let raw_case = format!("{}__{}", case.id, op.id);
        let inv = run_and_capture(
            side,
            RUNNER_BINARY,
            &raw_case,
            program,
            &args,
            &workspace,
            exec_kind(&op.subcommand).bound(),
            cfg.report_root,
        )
        .await?;

        // For a Docker op, snapshot the container at THIS op's boundary so the observers
        // (final state) and the invariant/metamorphic oracle (state ACROSS ops, US6) can
        // both read it. The final `up`'s container id is what the channel observers use.
        if docker_case && matches!(op.subcommand.as_str(), "up" | "exec") && inv.success {
            // Capture EVERY container matching this op's workspace label (not just one), so
            // the metamorphic oracle can detect a non-idempotent op that left a second
            // container behind (finding #3). Both docker probes run via `spawn_blocking` so
            // they never block the async executor (finding #4).
            let ws_for_lookup = workspace.clone();
            let this_ids =
                tokio::task::spawn_blocking(move || containers_for_workspace(&ws_for_lookup))
                    .await
                    .map_err(blocking_join_err)?;
            let this_id = this_ids.first().cloned();
            let inspect = match this_id.clone() {
                Some(id) => {
                    tokio::task::spawn_blocking(move || crate::observe::docker_inspect(&id))
                        .await
                        .map_err(blocking_join_err)??
                }
                None => None,
            };
            let temporal = inspect
                .as_ref()
                .map(crate::observe::temporal::temporal_from_inspect)
                .unwrap_or(serde_json::Value::Null);
            op_snapshots.push((
                op.id.clone(),
                crate::observe::OpSnapshot {
                    container_id: this_id.clone(),
                    container_ids: this_ids,
                    temporal,
                },
            ));
            if op.subcommand == "up" {
                // The final `up`'s container + its inspect are what the channel observers use.
                container_id = this_id;
                container_inspect = inspect;
            }
        }

        let failure_phase = if inv.success {
            None
        } else {
            Some(cli_process::infer_failure_phase(&op.subcommand))
        };
        outcomes.push((
            op.id.clone(),
            ProcessOutcome {
                exit_code: inv.exit_code,
                success: inv.success,
                stdout: inv.stdout,
                stderr: inv.stderr,
                failure_phase,
            },
        ));
    }

    let workspace = context_workspace.unwrap_or_else(|| cfg.fixtures_root.to_path_buf());
    let mut ctx = RunContext::new(workspace);
    // Scope the filesystem observer to the case's declared allowlist (clarify Q1).
    ctx.fs_allowlist = case.fs_allowlist.clone();
    ctx.container_id = container_id;
    ctx.container_inspect = container_inspect;
    for (op_id, outcome) in outcomes {
        ctx.record_outcome(op_id, outcome);
    }
    for (op_id, snapshot) in op_snapshots {
        ctx.record_op_snapshot(op_id, snapshot);
    }
    Ok((ctx, docker_ws))
}

/// Whether a case runs Docker-backed (its `resourceGroup` requests a Docker group). Such
/// cases get an isolated workspace + the RAII cleanup guard.
pub(crate) fn is_docker_case(case: &TestCase) -> bool {
    matches!(
        case.resource_group,
        Some(ResourceGroup::DockerShared) | Some(ResourceGroup::DockerExclusive)
    )
}

/// The de-duplicated, sorted fixture ids a case's operations reference.
fn unique_fixture_ids(case: &TestCase) -> Vec<String> {
    let mut ids: Vec<String> = case
        .operations
        .iter()
        .flat_map(|op| op.fixtures.iter().cloned())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Map a `spawn_blocking` join failure (the offloaded blocking task panicked) to a
/// fail-loud harness error. Used wherever the runner offloads a blocking docker probe so it
/// never blocks the async executor (finding #4).
pub(crate) fn blocking_join_err(e: tokio::task::JoinError) -> HarnessError {
    HarnessError::DockerUnavailable {
        cause: format!("a docker probe task failed to complete: {e}"),
    }
}

/// The pinned-image digests a case's fixtures declare — the `imageDigests` provenance /
/// staleness signal (FR-017, finding #5). A Docker case's snapshot MUST go stale when a
/// pinned image's content changes upstream, so this recomputes the digest of every image
/// the case's fixtures declare, at both record and replay time. Returns:
///
/// - `Some(empty)` for a NON-Docker case — it pulls no images, so its snapshot must NOT
///   depend on any image digest (a `read-configuration` case gating on the base image would
///   be the same false-staleness trap as gating on the host Node version); resolved without
///   touching docker.
/// - `Some(digests)` for a Docker case when `docker image inspect` resolves each declared
///   image (sorted, deduped for determinism).
/// - `None` for a Docker case when `docker` cannot be reached — the caller then carries the
///   RECORDED digests rather than fabricating (e.g. the hermetic `snapshot check`, which has
///   no docker: it cannot verify a Docker case's images and must not falsely flag them).
///
/// BLOCKING (it may shell out to `docker`); async callers offload it via `spawn_blocking`.
pub fn image_digests_for_case(
    case: &TestCase,
    fixtures_root: &Path,
) -> Option<Vec<(String, String)>> {
    if !is_docker_case(case) {
        return Some(Vec::new());
    }
    let mut out: Vec<(String, String)> = Vec::new();
    for id in unique_fixture_ids(case) {
        let Some(image) = fixture_image(&fixtures_root.join(&id)) else {
            continue; // Dockerfile/compose fixture (no top-level `image` ref) — nothing to pin.
        };
        match image_digest(&image) {
            Ok(Some(digest)) => out.push((image, digest)),
            // Image not present locally / no digest — best-effort, skip (not a docker fault).
            Ok(None) => {}
            // `docker` itself cannot run — cannot determine; the caller carries recorded.
            Err(()) => return None,
        }
    }
    out.sort();
    out.dedup();
    Some(out)
}

/// The `image` ref a fixture's devcontainer config declares, if any (mirrors the conformance
/// validator's `fixture_image`). A missing / unreadable / non-JSON config, or one with no
/// top-level `image`, yields `None`.
fn fixture_image(fixture_dir: &Path) -> Option<String> {
    for rel in [".devcontainer/devcontainer.json", ".devcontainer.json"] {
        let path = fixture_dir.join(rel);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if let Some(image) = doc.get("image").and_then(|v| v.as_str()) {
            return Some(image.to_string());
        }
    }
    None
}

/// The digest of a locally-available image `reference` via `docker image inspect`: its first
/// `RepoDigests` entry (the registry digest), else its content `.Id`. `Ok(None)` when the
/// image is absent locally / has neither; `Err(())` when `docker` itself cannot run.
fn image_digest(reference: &str) -> Result<Option<String>, ()> {
    let output = std::process::Command::new("docker")
        .args([
            "image",
            "inspect",
            reference,
            "--format",
            "{{if .RepoDigests}}{{index .RepoDigests 0}}{{else}}{{.Id}}{{end}}",
        ])
        .output()
        .map_err(|_| ())?;
    if !output.status.success() {
        // Non-zero is usually "No such image" (not pulled) — a not-present state, not a fault.
        return Ok(None);
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if s.is_empty() { None } else { Some(s) })
}

/// Find EVERY container deacon created for `workspace` via its `devcontainer.local_folder`
/// label (unique per isolated workspace, so collision-safe), sorted+deduped for a
/// deterministic result. Returning the full set — not just the first match — lets the
/// metamorphic oracle detect a non-idempotent op that left a second container behind
/// (finding #3). Empty when none is found or `docker` cannot run.
fn containers_for_workspace(workspace: &Path) -> Vec<String> {
    let ws = workspace.to_string_lossy();
    let Ok(output) = std::process::Command::new("docker")
        .args([
            "ps",
            "-aq",
            "--filter",
            &format!("label=devcontainer.local_folder={ws}"),
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let mut ids: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Which operation produced an expected observable: the explicit `operation`, else the
/// case's LAST operation (data-model §5).
pub(crate) fn resolve_expected_op<'a>(
    case: &'a TestCase,
    exp: &ExpectedObservable,
) -> Result<&'a Operation, HarnessError> {
    let target = match &exp.operation {
        Some(id) => case.operations.iter().find(|o| &o.id == id),
        None => case.operations.last(),
    };
    target.ok_or_else(|| {
        shape_error(
            case,
            &format!("expected channel {:?} refers to no operation", exp.channel),
        )
    })
}

/// Capture `exp`'s channel from `ctx` as BOTH raw and normalized evidence — the shared
/// step of spec-expectation and live-differential. Resolves the observer, captures raw,
/// then applies the named per-channel normalization rules with the workspace token map
/// (US3). Raw and normalized are returned separately (FR-016) so callers persist/compare
/// each independently.
pub(crate) fn capture_channel(
    case: &TestCase,
    exp: &ExpectedObservable,
    ctx: &RunContext,
) -> Result<(RawChannelEvidence, NormalizedChannelEvidence), HarnessError> {
    let op = resolve_expected_op(case, exp)?;
    let observer = observer_for(&exp.channel).ok_or_else(|| HarnessError::NormalizationFailed {
        channel: exp.channel.clone(),
        cause: "no observer for this channel yet (Docker channels land in US5)".to_string(),
    })?;
    let raw = observer.capture(ctx, op)?;
    let tokens = crate::normalize::TokenMap::workspace(&ctx.workspace);
    let normalized = crate::normalize::normalize_channel(&exp.channel, &raw, &tokens);
    Ok((raw, normalized))
}

/// Convenience: the normalized-only capture (spec-expectation / differential comparison
/// operate on normalized evidence).
pub(crate) fn capture_normalized(
    case: &TestCase,
    exp: &ExpectedObservable,
    ctx: &RunContext,
) -> Result<NormalizedChannelEvidence, HarnessError> {
    Ok(capture_channel(case, exp, ctx)?.1)
}

/// Run a case's operations against deacon and collect its [`CaseEvidence`] — raw and
/// normalized held SEPARATELY (FR-016) for every declared channel. Used by the
/// spec-expectation path and exposed so record/replay (US2) and tests can retrieve raw
/// and normalized independently.
pub async fn collect_spec_evidence(
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<crate::evidence::CaseEvidence, HarnessError> {
    // `_ws` (the RAII cleanup guard) is held until after every channel is captured, then
    // dropped to reclaim the container/network/volume/temp dir (FR-039).
    let (ctx, _ws) = execute_ops(Side::Deacon, cfg.deacon_path, case, cfg).await?;
    let mut evidence = crate::evidence::CaseEvidence::new();
    for exp in &case.expected {
        let (raw, normalized) = capture_channel(case, exp, &ctx)?;
        evidence.push(raw, normalized);
    }
    Ok(evidence)
}

/// Run a case's operations against the given `program` (`Side::Deacon` or
/// `Side::Oracle`) and collect its [`CaseEvidence`] — the record path for snapshots.
pub async fn collect_evidence_on(
    side: Side,
    program: &std::path::Path,
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<crate::evidence::CaseEvidence, HarnessError> {
    let (ctx, _ws) = execute_ops(side, program, case, cfg).await?;
    let mut evidence = crate::evidence::CaseEvidence::new();
    for exp in &case.expected {
        let (raw, normalized) = capture_channel(case, exp, &ctx)?;
        evidence.push(raw, normalized);
    }
    Ok(evidence)
}

/// Build the 13-field [`Provenance`] for a snapshot recording (T035, data-model §7):
/// recompute the case/fixture hashes, take the oracle version from the verified oracle,
/// probe Node/Docker/Compose versions (via the shared
/// [`deacon_conformance::snapshot::probe_environment`]), and stamp the source revision +
/// normalizer version. `imageDigests` records the digest of each image a Docker case's
/// fixtures pin ([`image_digests_for_case`]) so the snapshot goes stale if a pinned image's
/// content changes; it is empty for config-only cases (they pull no images).
///
/// Provenance fields are recorded verbatim from the environment — NEVER fabricated
/// (constitution IV). A missing Node/Docker/Compose tool records an empty string (the
/// refresh bin fail-loud-checks Docker/Node presence before calling this, so in practice
/// they are always present at record time).
pub fn capture_provenance(
    case: &TestCase,
    cfg: &RunConfig<'_>,
    oracle_version: &str,
) -> Result<deacon_conformance::snapshot::Provenance, HarnessError> {
    use deacon_conformance::snapshot;

    let (case_hash, fixture_hash) = snapshot_hashes(case, cfg)?;
    let env = snapshot::probe_environment();

    Ok(deacon_conformance::snapshot::Provenance {
        oracle_version: oracle_version.to_string(),
        source_revision: deacon_conformance::CURRENT_SPEC_PIN.to_string(),
        case_hash,
        fixture_hash,
        argv: tokenized_argv(case),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        node_version: env.node_version.unwrap_or_default(),
        docker_version: env.docker_version.unwrap_or_default(),
        compose_version: env.compose_version.unwrap_or_default(),
        // Digests of the images a Docker case pins (empty for config-only cases); `.collect()`
        // infers the `IndexMap` field type (finding #5).
        image_digests: image_digests_for_case(case, cfg.fixtures_root)
            .unwrap_or_default()
            .into_iter()
            .collect(),
        normalizer_version: crate::normalize::NORMALIZER_VERSION.to_string(),
        captured_at: crate::report::now_rfc3339(),
    })
}

/// Recompute `(caseHash, fixtureHash)` for `case`, mapping the shared conformance helper's
/// IO error to a fail-loud [`HarnessError`].
pub fn snapshot_hashes(
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<(String, String), HarnessError> {
    deacon_conformance::case_hash::hashes_for_case(case, cfg.fixtures_root).map_err(|e| {
        HarnessError::FixtureMissing {
            path: cfg.fixtures_root.join(format!("<case {}>: {e}", case.id)),
        }
    })
}

/// The primary operation's argv (`[subcommand] ++ argv`) with `${WORKSPACE}` tokenized to
/// `<WORKSPACE>` — the portable argv recorded in provenance (contract snapshot-provenance.md).
fn tokenized_argv(case: &TestCase) -> Vec<String> {
    let Some(op) = case.operations.first() else {
        return Vec::new();
    };
    let mut argv = vec![op.subcommand.clone()];
    for a in &op.argv {
        argv.push(a.replace(WORKSPACE_TOKEN, "<WORKSPACE>"));
    }
    argv
}

/// Attach the failure phase to the `chan-exit-code` verdict's detail when the producing
/// operation failed (FR-009). Path-free and deterministic, so the report stays
/// byte-stable (T018).
pub(crate) fn attach_failure_phase(
    verdict: &mut ChannelVerdict,
    case: &TestCase,
    exp: &ExpectedObservable,
    ctx: &RunContext,
) {
    if verdict.channel != CHAN_EXIT_CODE {
        return;
    }
    let Ok(op) = resolve_expected_op(case, exp) else {
        return;
    };
    let Some(phase) = ctx.outcome(&op.id).and_then(|o| o.failure_phase) else {
        return;
    };
    let phase_value = serde_json::to_value(phase).unwrap_or(serde_json::Value::Null);
    match verdict.detail.as_mut() {
        Some(serde_json::Value::Object(map)) => {
            map.insert("failurePhase".to_string(), phase_value);
        }
        _ => {
            verdict.detail = Some(serde_json::json!({ "failurePhase": phase_value }));
        }
    }
}

/// The per-invocation time bound class for a subcommand (config-only vs lifecycle).
fn exec_kind(subcommand: &str) -> ExecKind {
    match subcommand {
        "read-configuration" | "doctor" => ExecKind::Config,
        _ => ExecKind::Lifecycle,
    }
}

/// Resolve the workspace an operation runs against. US1 supports a single fixture id
/// mapping to `<fixtures_root>/<id>/`; zero fixtures runs against `fixtures_root`
/// itself. Multiple fixtures per op (merged into one isolated workspace) is US5 —
/// fail-loud until then rather than silently pick one.
fn resolve_workspace(
    case: &TestCase,
    op: &Operation,
    cfg: &RunConfig<'_>,
) -> Result<PathBuf, HarnessError> {
    match op.fixtures.as_slice() {
        [] => Ok(cfg.fixtures_root.to_path_buf()),
        [one] => {
            let dir = cfg.fixtures_root.join(one);
            if dir.is_dir() {
                Ok(dir)
            } else {
                Err(HarnessError::FixtureMissing { path: dir })
            }
        }
        _ => Err(shape_error(
            case,
            &format!(
                "operation {:?} references {} fixtures; multi-fixture workspaces land in US5",
                op.id,
                op.fixtures.len()
            ),
        )),
    }
}

/// Substitute `${WORKSPACE}` in an operation's argv with the resolved workspace path. An
/// argv that references the token with no resolvable fixture is a fail-loud authoring
/// error.
fn substitute_argv(
    case: &TestCase,
    op: &Operation,
    workspace: &Path,
    workspace_is_rooted: bool,
) -> Result<Vec<String>, HarnessError> {
    let ws = workspace.to_string_lossy();
    let mut out = Vec::with_capacity(op.argv.len());
    for arg in &op.argv {
        // A config-only op that uses `${WORKSPACE}` must declare a fixture to root the
        // token; a Docker op shares the already-rooted isolated workspace.
        if arg.contains(WORKSPACE_TOKEN) && op.fixtures.is_empty() && !workspace_is_rooted {
            return Err(shape_error(
                case,
                &format!(
                    "operation {:?} uses {WORKSPACE_TOKEN} but declares no fixture to root it",
                    op.id
                ),
            ));
        }
        out.push(arg.replace(WORKSPACE_TOKEN, &ws));
    }
    Ok(out)
}

/// A fail-loud case-shape / authoring error, surfaced as a normalization-class failure
/// so it carries the case id in its channel slot.
fn shape_error(case: &TestCase, cause: &str) -> HarnessError {
    HarnessError::NormalizationFailed {
        channel: format!("case:{}", case.id),
        cause: cause.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_conformance::model::OracleType;

    fn case_with_op(argv: &[&str], fixtures: &[&str]) -> TestCase {
        TestCase {
            id: "case-x".to_string(),
            oracle_type: Some(OracleType::SpecExpectation),
            operations: vec![Operation {
                id: "op-1".to_string(),
                subcommand: "read-configuration".to_string(),
                argv: argv.iter().map(|s| s.to_string()).collect(),
                fixtures: fixtures.iter().map(|s| s.to_string()).collect(),
                ..Operation::default()
            }],
            ..TestCase::default()
        }
    }

    #[test]
    fn substitute_argv_requires_a_fixture_for_the_token() {
        let case = case_with_op(&["--workspace-folder", "${WORKSPACE}"], &[]);
        // Config-only (not rooted) + no fixture → fail loud.
        let err = substitute_argv(&case, &case.operations[0], Path::new("/tmp/ws"), false)
            .expect_err("token with no fixture must fail loud");
        assert!(matches!(err, HarnessError::NormalizationFailed { .. }));
        // But a rooted (isolated Docker) workspace resolves the token even with no fixture.
        let ok = substitute_argv(&case, &case.operations[0], Path::new("/tmp/ws"), true)
            .expect("rooted workspace resolves the token");
        assert_eq!(ok, vec!["--workspace-folder", "/tmp/ws"]);
    }

    #[test]
    fn substitute_argv_replaces_token() {
        let case = case_with_op(&["--workspace-folder", "${WORKSPACE}"], &["fx-x"]);
        let out = substitute_argv(&case, &case.operations[0], Path::new("/tmp/ws"), false).unwrap();
        assert_eq!(out, vec!["--workspace-folder", "/tmp/ws"]);
    }

    #[test]
    fn exec_kind_classifies_config_only() {
        assert_eq!(exec_kind("read-configuration"), ExecKind::Config);
        assert_eq!(exec_kind("up"), ExecKind::Lifecycle);
    }

    #[test]
    fn image_digests_for_config_only_case_is_empty_without_docker() {
        // A non-Docker case pulls no images → `Some(empty)`, resolved WITHOUT touching
        // docker, so a read-configuration snapshot never gates on a base-image digest
        // (finding #5). The path is nonexistent to prove no fixture/docker access happens.
        let case = case_with_op(&[], &[]);
        assert_eq!(
            image_digests_for_case(&case, Path::new("/nonexistent")),
            Some(Vec::new())
        );
    }

    #[test]
    fn fixture_image_reads_declared_image_else_none() {
        let dir = tempfile::tempdir().unwrap();
        let dc = dir.path().join("fx/.devcontainer");
        std::fs::create_dir_all(&dc).unwrap();
        std::fs::write(
            dc.join("devcontainer.json"),
            r#"{ "image": "alpine:3.19" }"#,
        )
        .unwrap();
        assert_eq!(
            fixture_image(&dir.path().join("fx")).as_deref(),
            Some("alpine:3.19")
        );
        // A fixture with no top-level image (Dockerfile/compose) → None.
        let dc2 = dir.path().join("fx2/.devcontainer");
        std::fs::create_dir_all(&dc2).unwrap();
        std::fs::write(dc2.join("devcontainer.json"), r#"{ "name": "x" }"#).unwrap();
        assert_eq!(fixture_image(&dir.path().join("fx2")), None);
    }

    #[test]
    fn resolve_expected_op_defaults_to_last() {
        let mut case = case_with_op(&[], &[]);
        case.operations.push(Operation {
            id: "op-2".to_string(),
            subcommand: "read-configuration".to_string(),
            ..Operation::default()
        });
        let exp = ExpectedObservable {
            channel: CHAN_EXIT_CODE.to_string(),
            operation: None,
            assertion: None,
        };
        assert_eq!(resolve_expected_op(&case, &exp).unwrap().id, "op-2");
    }
}
