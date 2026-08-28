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
use std::time::Duration;

use crate::model::{
    CHAN_EXIT_CODE, CaseKind, ExpectedObservable, Operation, ResourceGroup, TestCase,
};

use crate::HarnessError;
use crate::evidence::{
    CaseVerdict, ChannelVerdict, NormalizedChannelEvidence, Outcome, RawChannelEvidence,
};
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

/// The `${IMAGE_TAG}` token substituted in an operation's argv with a collision-resistant
/// image name unique to this case run AND this side.
///
/// A `build` produces an image and no container, so the only handle the harness has on
/// what was produced is the tag the operation was told to write. Rather than parse it back
/// out of each CLI's stdout — two different JSON shapes, and absent entirely in text mode —
/// the case declares `--image-name ${IMAGE_TAG}` and the runner both resolves the token and
/// tracks the resulting tag for reclamation.
///
/// Per-side uniqueness is the point, not an accident: deacon and the reference build
/// SEPARATELY, so a shared tag would have the second build overwrite the first and the
/// comparison would silently be an image against itself.
const IMAGE_TAG_TOKEN: &str = "${IMAGE_TAG}";

/// The `${CONTAINER_ID}` token substituted with the id of the container an earlier `up`
/// operation in the same case created.
///
/// Exists for one shape of claim the declarative model could not otherwise make: a
/// subcommand addressed by CONTAINER rather than by workspace (`exec --container-id …`
/// with no `--workspace-folder` and no `--config`). Such an operation must recover what it
/// needs from the container itself — from the `devcontainer.metadata` label `up` stamped —
/// and there is no way to write that case without naming a container the case did not know
/// about when it was authored.
///
/// Resolved from the ids observed so far, so the token only works in an operation that
/// FOLLOWS the `up`; using it earlier fails loud rather than expanding to nothing.
const CONTAINER_ID_TOKEN: &str = "${CONTAINER_ID}";

/// The per-case wall-clock bound every declarative case runs under (024 FR-077b).
///
/// This is deliberately NOT nextest's `slow-timeout`, which is per TEST FUNCTION. A driver
/// function owns a whole resource group, so a `slow-timeout` failure says only "the group
/// was slow" — indistinguishable from a wedged daemon, and naming nothing to fix. Bounding
/// each case here fails with the CASE ID instead, and lets the remaining cases of the group
/// still run and still report.
///
/// It bounds the case END TO END — every operation, both sides of a differential, and every
/// observation — which is strictly wider than [`crate::exec`]'s per-invocation bound. A case
/// that hangs BETWEEN invocations (a teardown that never returns, a snapshot probe against an
/// unresponsive daemon) is invisible to the per-invocation bound and is exactly what this
/// catches.
pub const CASE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

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
}

/// Run one declarative case end to end and produce its [`CaseVerdict`], bounded by
/// [`CASE_TIMEOUT`].
///
/// The bound wraps the ENTIRE case, so expiry is reported as
/// [`HarnessError::CaseTimeout`] naming the case id (FR-077b) rather than surfacing as an
/// unattributable stall in whichever driver loop invoked it. Abandoning the evaluation
/// future drops it, which runs the Docker workspace's RAII cleanup guard — a timed-out case
/// still reclaims its container, network, volume and temp directory.
pub async fn run_case(case: &TestCase, cfg: &RunConfig<'_>) -> Result<CaseVerdict, HarnessError> {
    match tokio::time::timeout(CASE_TIMEOUT, run_case_unbounded(case, cfg)).await {
        Ok(result) => result,
        Err(_elapsed) => Err(HarnessError::CaseTimeout {
            case: case.id.clone(),
            bound: CASE_TIMEOUT,
        }),
    }
}

/// [`run_case`] without the per-case bound — the body the timeout wraps.
async fn run_case_unbounded(
    case: &TestCase,
    cfg: &RunConfig<'_>,
) -> Result<CaseVerdict, HarnessError> {
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
    // every resource on success AND unwind.
    //
    // EVERY other case gets the same isolation, reclaiming the temp dir only — the
    // config-only lanes are defined to need no daemon, so their cleanup must not shell
    // out to one.
    //
    // That used to be true of `fs-heavy` alone, on the reasoning that its group means
    // "significant filesystem operations" and those must not land in `parity/fixtures/`,
    // which is version-controlled input every other case reads. The rest ran against the
    // committed fixture directory DIRECTLY, on the assumption that they were read-only —
    // an assumption nothing enforced and that #680 disproved: a `build` that got past a
    // policy gate resolved a Feature and wrote a `devcontainer-lock.json` into the
    // repository. "Significant filesystem operations" was never the property that
    // mattered; writing at all was, and a case cannot declare in advance that the CLI
    // will not write. Isolation is now unconditional and `FixtureIntegrity` proves it.
    //
    // Behavior-preserving when it landed: no non-Docker case names more than one distinct
    // fixture across its operations, and none has an operation with no fixture, so
    // layering every fixture into one workspace merges nothing and orphans nothing.
    let docker_case = is_docker_case(case);
    // An operation naming more than one fixture is still a shape error. The materialize
    // step below layers `unique_fixture_ids(case)` into one workspace and would silently
    // accept it, so the rule is checked explicitly rather than falling out of the
    // per-operation workspace resolution it used to live in.
    for op in &case.operations {
        if op.fixtures.len() > 1 {
            return Err(shape_error(
                case,
                &format!(
                    "operation {:?} references {} fixtures; one operation names one fixture",
                    op.id,
                    op.fixtures.len()
                ),
            ));
        }
    }
    let docker_ws: DockerWorkspace;
    let isolated_workspace: PathBuf = {
        // Creating the temp dir and recursively copying every fixture tree into it is
        // BLOCKING filesystem work. Under the bounded-concurrency Docker driver (T018)
        // several cases set up at once, so doing it inline would stall the executor for
        // every other in-flight case (Principle V) — offload it exactly as the docker
        // probes below already are.
        let fixture_dirs: Vec<PathBuf> = unique_fixture_ids(case)
            .into_iter()
            .map(|id| cfg.fixtures_root.join(id))
            .collect();
        let deacon_path = cfg.deacon_path.to_path_buf();
        let ws = tokio::task::spawn_blocking(move || -> Result<DockerWorkspace, HarnessError> {
            let ws = if docker_case {
                DockerWorkspace::new(Some(&deacon_path))
            } else {
                DockerWorkspace::new_filesystem_only()
            }
            .map_err(|e| HarnessError::DockerUnavailable {
                cause: format!("could not create an isolated workspace: {e}"),
            })?;
            for dir in fixture_dirs {
                if !dir.is_dir() {
                    // `ws` drops here, ON THE BLOCKING POOL, reclaiming the temp dir.
                    return Err(HarnessError::FixtureMissing { path: dir });
                }
                ws.materialize(&dir)
                    .map_err(|e| HarnessError::FixtureMissing {
                        path: dir.join(format!("<materialize failed: {e}>")),
                    })?;
            }
            Ok(ws)
        })
        .await
        .map_err(blocking_join_err)??;
        let path = ws.path().to_path_buf();
        docker_ws = ws;
        path
    };

    // Fingerprint the committed fixture trees so a case that somehow writes into them is
    // caught here rather than found later as an unexplained modified file (#680). With
    // isolation unconditional this should never fire — which is the point: the copy is
    // the fix, and this is the proof the copy is working.
    let fixture_guard =
        crate::workspace::FixtureIntegrity::capture(cfg.fixtures_root, &unique_fixture_ids(case))
            .map_err(|e| HarnessError::FixtureMissing {
            path: cfg
                .fixtures_root
                .join(format!("<could not fingerprint fixtures: {e}>")),
        })?;

    // The tag a `build` operation writes to, when the case asks for one. Registered for
    // reclamation up front so an image survives no longer than its case even if the run
    // panics between the build and the inspect.
    let mut docker_ws = docker_ws;
    let image_tag: Option<String> = if docker_case {
        let tag = format!("{}:latest", docker_ws.resource_name("img"));
        docker_ws.track_image(tag.clone());
        Some(tag)
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
    // How many containers the workspace held after the final `up`, and how many were live
    // (#371) — captured from the same probes that pick the observed container.
    let mut workspace_container_census: Option<crate::observe::WorkspaceContainerCensus> = None;
    // The final successful `build`'s image inspect, captured the same way.
    let mut image_inspect: Option<serde_json::Value> = None;

    for op in &case.operations {
        // Every case runs in its own isolated workspace, shared across operations (#680).
        let workspace = isolated_workspace.clone();
        if context_workspace.is_none() {
            context_workspace = Some(workspace.clone());
        }
        // For a Docker case every op shares the ISOLATED workspace (materialized once), so
        // `${WORKSPACE}` always resolves even for a later op that declares no fixture.
        let argv = substitute_argv(
            case,
            op,
            &workspace,
            true,
            image_tag.as_deref(),
            container_id.as_deref(),
        )?;
        let mut full: Vec<String> = subcommand_tokens(&op.subcommand);
        full.extend(argv);
        let args: Vec<&str> = full.iter().map(String::as_str).collect();

        // Resolved against the workspace, so the payload arrives the way every other
        // byte-exact input does: as a fixture file this op already materialized (#586).
        let stdin_file = op.stdin_file.as_ref().map(|rel| workspace.join(rel));

        // The one container-lifecycle primitive (#480): stop this side's running containers
        // so THIS operation runs against a stopped container rather than a fresh one. It
        // happens after argv substitution and before the spawn, which is what makes the
        // restart and the operation that performs it one declared step.
        if op.stop_container_before {
            let ws_for_stop = workspace.clone();
            let case_id = case.id.clone();
            tokio::task::spawn_blocking(move || stop_running_containers(&case_id, &ws_for_stop))
                .await
                .map_err(blocking_join_err)??;
        }

        // Give this side a private TMPDIR before anything the case declares, so a case
        // that deliberately sets one still wins (`cmd.envs` applies in order).
        //
        // Without it the ORACLE is not isolated from its concurrent siblings: the
        // reference CLI stages its generated Dockerfile under a path keyed only by
        // version and `Date.now()`, so two invocations in the same millisecond share it
        // and one builds the other's Dockerfile — measured as a phantom divergence on
        // `case-build-failure-reported` (#721). deacon is not affected (its own build
        // temp dir carries the pid), but both sides are isolated here anyway: the two
        // must run in comparable worlds, and treating only one specially would be its
        // own defect.
        let mut env = vec![];
        let side_tmp =
            docker_ws
                .side_tmpdir(side.name())
                .map_err(|e| HarnessError::DockerUnavailable {
                    cause: format!("could not create a private TMPDIR for {}: {e}", side.name()),
                })?;
        let side_tmp = side_tmp.to_string_lossy().into_owned();
        // TMPDIR is what Node's `os.tmpdir()` and Rust's `std::env::temp_dir()` both read
        // on Unix; TMP/TEMP are set alongside so a Windows lane inherits the same
        // isolation rather than silently keeping the shared path.
        for key in ["TMPDIR", "TMP", "TEMP"] {
            env.push((key.to_string(), side_tmp.clone()));
        }
        env.extend(substitute_env(case, op, &workspace, true)?);

        let raw_case = format!("{}__{}", case.id, op.id);
        let inv = run_and_capture(
            side,
            RUNNER_BINARY,
            &raw_case,
            program,
            &args,
            &workspace,
            stdin_file.as_deref(),
            &env,
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
                    .map_err(blocking_join_err)??;
            // The op SUCCEEDED, so a container must exist: finding none means the
            // observation is broken, not that there is nothing to see (D-2).
            require_observed_container(&case.id, &op.id, &this_ids, &workspace)?;
            // Which of them are RUNNING. Two things depend on knowing this, and both were
            // broken while a superseded container stayed live (#371).
            //
            // First, WHICH container the Docker channels observe. `this_ids` is sorted by
            // id — random hex — so picking `.first()` out of a multi-container workspace is
            // a coin flip between generations, and a draft of
            // `case-up-stale-config-reentry-differential` observing `chan-container-state`
            // after a recreate failed about one run in three for exactly that reason. #371
            // stops the superseded container but deliberately does NOT remove it, so the
            // ambiguity survives the fix and has to be resolved here: the live generation
            // is the one still running.
            //
            // Second, it is the census `chan-temporal` reports, which is how a case asserts
            // "exactly one container for this workspace is live" rather than assuming it.
            let ws_for_running = workspace.clone();
            let running_ids = tokio::task::spawn_blocking(move || {
                running_containers_for_workspace(&ws_for_running)
            })
            .await
            .map_err(blocking_join_err)??;
            let workspace_containers = Some(crate::observe::WorkspaceContainerCensus {
                total: this_ids.len(),
                running: running_ids.len(),
            });
            let this_id = running_ids.first().or_else(|| this_ids.first()).cloned();
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
                workspace_container_census = workspace_containers;
            }
        }

        // A `build` leaves an IMAGE and no container, so the container probe above sees
        // nothing. Inspect the tag the operation was told to write instead — the only
        // handle on what a build produced. A missing image after a SUCCESSFUL build is a
        // broken observation, not an empty one (D-2), so it fails loud rather than
        // recording `present:false`.
        //
        // Gated on the operation actually ASKING for the tag. A `build` case that declares
        // no `chan-image` expectation writes wherever its configuration says and has no
        // reason to name a tag; inspecting one it never wrote would fail every such case on
        // an image that was never supposed to exist.
        let asked_for_tag = op.argv.iter().any(|a| a.contains(IMAGE_TAG_TOKEN));
        if docker_case && op.subcommand == "build" && inv.success && asked_for_tag {
            if let Some(tag) = image_tag.clone() {
                let inspected =
                    tokio::task::spawn_blocking(move || crate::observe::docker_inspect(&tag))
                        .await
                        .map_err(blocking_join_err)??;
                let Some(inspected) = inspected else {
                    return Err(shape_error(
                        case,
                        &format!(
                            "operation {:?} built successfully but no image exists at {:?} — \
                             the case must pass `--image-name {IMAGE_TAG_TOKEN}` for the built \
                             image to be observable",
                            op.id,
                            image_tag.as_deref().unwrap_or("<none>")
                        ),
                    ));
                };
                image_inspect = Some(inspected);
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

    // Checked after the operations and before any evidence is assembled, so a case that
    // mutated version-controlled input fails on THAT rather than on whatever its
    // assertions happened to see afterwards.
    if let Some(problem) =
        fixture_guard
            .verify()
            .map_err(|e| HarnessError::NormalizationFailed {
                channel: format!("fixture-integrity[{}]", case.id),
                cause: format!("could not re-fingerprint the fixture trees: {e}"),
            })?
    {
        return Err(HarnessError::NormalizationFailed {
            channel: format!("fixture-integrity[{}]", case.id),
            cause: problem,
        });
    }

    let workspace = context_workspace.unwrap_or_else(|| cfg.fixtures_root.to_path_buf());
    let mut ctx = RunContext::for_side(workspace, side);
    // Scope the filesystem observer to the case's declared allowlist (clarify Q1).
    ctx.fs_allowlist = case.fs_allowlist.clone();
    ctx.container_id = container_id;
    ctx.container_inspect = container_inspect;
    ctx.workspace_containers = workspace_container_census;
    ctx.image_inspect = image_inspect;
    ctx.image_tag = image_tag;
    for (op_id, outcome) in outcomes {
        ctx.record_outcome(op_id, outcome);
    }
    for (op_id, snapshot) in op_snapshots {
        ctx.record_op_snapshot(op_id, snapshot);
    }

    Ok((ctx, Some(docker_ws)))
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

/// Reclaim a Docker case's workspace OFF the async executor, then drop it there too.
///
/// Reclamation shells out to `deacon down` plus several `docker` calls, all via blocking
/// `std::process::Command::output`, and then removes the temp tree. Letting the guard drop
/// inline stalls the runtime for seconds — per case, per side — which under the bounded-
/// concurrency Docker driver (T018) stalls every OTHER in-flight case too (Principle V).
///
/// `cleanup_now` is idempotent and the guard's `Drop` runs inside the blocking task, so the
/// temp directory is removed there as well. `None` (a config-only case) is a no-op.
///
/// The error path is deliberately left to `Drop`: an early `?` return abandons the guard,
/// which still reclaims — synchronously, on the executor — because a leaked container is
/// worse than a stalled runtime on a path that is already failing.
pub(crate) async fn release_workspace(ws: Option<DockerWorkspace>) -> Result<(), HarnessError> {
    let Some(mut ws) = ws else {
        return Ok(());
    };
    tokio::task::spawn_blocking(move || ws.cleanup_now())
        .await
        .map_err(blocking_join_err)
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

/// The container-runtime CLI the runner probes with.
const DOCKER_BIN: &str = "docker";

/// Find EVERY container deacon created for `workspace` via its `devcontainer.local_folder`
/// label (unique per isolated workspace, so collision-safe), sorted+deduped for a
/// deterministic result. Returning the full set — not just the first match — lets the
/// metamorphic oracle detect a non-idempotent op that left a second container behind
/// (finding #3).
///
/// A `docker ps` FAULT (unspawnable CLI, non-zero exit) is a cause-specific
/// [`HarnessError::DockerUnavailable`], never an empty vec (024 Phase 3, D-2): swallowing
/// it turned a daemon hiccup into `not-captured` on every Docker channel, which the
/// differential then read as agreement — a silent green pass (constitution IV). An empty
/// `Ok` means the probe RAN and matched nothing, which is a different claim entirely.
///
/// BLOCKING (shells out to `docker`); async callers offload it via `spawn_blocking`.
fn containers_for_workspace(workspace: &Path) -> Result<Vec<String>, HarnessError> {
    containers_for_workspace_with(DOCKER_BIN, workspace)
}

/// [`containers_for_workspace`] restricted to containers that are RUNNING.
///
/// A separate probe rather than a state field on the existing one: `docker ps` filters
/// server-side, so this is one more cheap call instead of an inspect per candidate.
///
/// BLOCKING; async callers offload it via `spawn_blocking`.
fn running_containers_for_workspace(workspace: &Path) -> Result<Vec<String>, HarnessError> {
    running_containers_for_workspace_with(DOCKER_BIN, workspace)
}

/// [`running_containers_for_workspace`] with an injectable container-CLI program — the
/// same seam [`containers_for_workspace_with`] exposes for the hermetic fault tests.
pub fn running_containers_for_workspace_with(
    docker: &str,
    workspace: &Path,
) -> Result<Vec<String>, HarnessError> {
    // `ps` without `-a` lists running containers only.
    workspace_container_ids(docker, workspace, false)
}

/// Stop every RUNNING container carrying `workspace`'s `devcontainer.local_folder` label,
/// for `Operation::stop_container_before` (#480).
///
/// STOP, not remove: the claim this exists for is that a completed `postCreateCommand` does
/// not re-run when the container comes back up, and a removed container is recreated, which
/// re-runs it by definition. `docker stop` leaves the container and its filesystem intact so
/// the next `up` reattaches to the same id.
///
/// Scoped by the SIDE'S OWN workspace label, so the deacon and oracle passes — which run in
/// sequence over two distinct temp workspaces — cannot stop each other's containers.
///
/// Stopping nothing is a FAULT, not a no-op (the D-2 rule the workspace probes already
/// follow): a case declaring this flag is asserting a restart, so a probe that matched no
/// running container means the restart never happened and every downstream assertion would
/// be measuring a first create while looking like it measured a restart.
///
/// BLOCKING; async callers offload it via `spawn_blocking`.
fn stop_running_containers(case: &str, workspace: &Path) -> Result<Vec<String>, HarnessError> {
    stop_running_containers_with(DOCKER_BIN, case, workspace)
}

/// [`stop_running_containers`] with an injectable container-CLI program — the same seam
/// [`containers_for_workspace_with`] exposes for the hermetic fault tests.
pub fn stop_running_containers_with(
    docker: &str,
    case: &str,
    workspace: &Path,
) -> Result<Vec<String>, HarnessError> {
    let ids = running_containers_for_workspace_with(docker, workspace)?;
    if ids.is_empty() {
        return Err(HarnessError::ObservationFault {
            case: case.to_string(),
            cause: format!(
                "`stopContainerBefore` found no RUNNING container labelled \
                 devcontainer.local_folder={}; the operation would have measured a first \
                 create rather than a restart",
                workspace.display()
            ),
        });
    }
    let mut args: Vec<&str> = vec!["stop"];
    args.extend(ids.iter().map(String::as_str));
    let shown = args.join(" ");
    let output = std::process::Command::new(docker)
        .args(&args)
        .output()
        .map_err(|e| HarnessError::DockerUnavailable {
            cause: format!("could not run `docker {shown}`: {e}"),
        })?;
    if !output.status.success() {
        return Err(HarnessError::DockerUnavailable {
            cause: format!(
                "`docker {shown}` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(ids)
}

/// [`containers_for_workspace`] with an injectable container-CLI program — the seam the
/// hermetic fault tests drive (a stub that cannot spawn / exits non-zero / prints ids), so
/// the fault path is demonstrated without a Docker daemon or process-wide `PATH` mutation.
pub fn containers_for_workspace_with(
    docker: &str,
    workspace: &Path,
) -> Result<Vec<String>, HarnessError> {
    workspace_container_ids(docker, workspace, true)
}

/// The shared body of the two workspace probes: `docker ps [-a] -q --filter
/// label=devcontainer.local_folder=<ws>`, sorted and deduped. `all` selects `-a`
/// (every state) versus running-only.
fn workspace_container_ids(
    docker: &str,
    workspace: &Path,
    all: bool,
) -> Result<Vec<String>, HarnessError> {
    let ws = workspace.to_string_lossy();
    let filter = format!("label=devcontainer.local_folder={ws}");
    let mut args: Vec<&str> = vec!["ps"];
    if all {
        args.push("-a");
    }
    args.extend(["-q", "--filter", &filter]);
    let shown = args.join(" ");
    let output = std::process::Command::new(docker)
        .args(&args)
        .output()
        .map_err(|e| HarnessError::DockerUnavailable {
            cause: format!("could not run `docker {shown}`: {e}"),
        })?;
    if !output.status.success() {
        return Err(HarnessError::DockerUnavailable {
            cause: format!(
                "`docker {shown}` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let mut ids: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// A SUCCESSFUL Docker operation must leave a discoverable container. Finding none is an
/// observation fault, not an absence of state (024 Phase 3, D-2): the container the op
/// certainly created was not discovered by its workspace label, so every Docker channel
/// would report `not-captured` and the differential would vacuously agree. Fail loud.
pub fn require_observed_container(
    case_id: &str,
    op_id: &str,
    ids: &[String],
    workspace: &Path,
) -> Result<(), HarnessError> {
    if !ids.is_empty() {
        return Ok(());
    }
    Err(HarnessError::ObservationFault {
        case: case_id.to_string(),
        cause: format!(
            "operation {op_id:?} succeeded but no container carries the label \
             `devcontainer.local_folder={}` — the container it created was not discovered, \
             so every Docker channel would report not-captured",
            workspace.to_string_lossy()
        ),
    })
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
    // The token policy is per-channel and lives in the normalizer, not here (Constitution
    // VIII): `chan-container-state` also tokenizes the workspace BASENAME, since each side
    // runs in its own temp workspace and the container-side paths carry only that name.
    let tokens = crate::normalize::tokens_for_channel(
        &exp.channel,
        &ctx.workspace,
        ctx.image_tag.as_deref(),
    );
    let normalized = crate::normalize::normalize_channel(&exp.channel, &raw, &tokens, ctx.side);
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
    // `ws` (the RAII cleanup guard) is held until after every channel is captured, then
    // released to reclaim the container/network/volume/temp dir (FR-039) — off the async
    // executor, since reclamation shells out to `deacon down` + several `docker` calls.
    let (ctx, ws) = execute_ops(Side::Deacon, cfg.deacon_path, case, cfg).await?;
    let mut evidence = crate::evidence::CaseEvidence::new();
    for exp in &case.expected {
        let (raw, normalized) = capture_channel(case, exp, &ctx)?;
        evidence.push(raw, normalized);
    }
    release_workspace(ws).await?;
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
    let (ctx, ws) = execute_ops(side, program, case, cfg).await?;
    let mut evidence = crate::evidence::CaseEvidence::new();
    for exp in &case.expected {
        let (raw, normalized) = capture_channel(case, exp, &ctx)?;
        evidence.push(raw, normalized);
    }
    release_workspace(ws).await?;
    Ok(evidence)
}

/// Build the 13-field [`Provenance`] for a snapshot recording (T035, data-model §7):
/// recompute the case/fixture hashes, take the oracle version from the verified oracle,
/// probe Node/Docker/Compose versions (via the shared
/// [`crate::provenance::probe_environment`]), and stamp the source revision +
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
) -> Result<crate::provenance::Provenance, HarnessError> {
    use crate::provenance;

    let (case_hash, fixture_hash) = snapshot_hashes(case, cfg)?;
    let env = provenance::probe_environment();

    Ok(crate::provenance::Provenance {
        oracle_version: oracle_version.to_string(),
        source_revision: crate::CURRENT_SPEC_PIN.to_string(),
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
    crate::case_hash::hashes_for_case(case, cfg.fixtures_root).map_err(|e| {
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
    let mut argv = subcommand_tokens(&op.subcommand);
    for a in &op.argv {
        argv.push(
            a.replace(WORKSPACE_TOKEN, "<WORKSPACE>")
                .replace(IMAGE_TAG_TOKEN, "<IMAGE_TAG>")
                .replace(CONTAINER_ID_TOKEN, "<CONTAINER_ID>"),
        );
    }
    argv
}

/// The command-line tokens a declared `subcommand` expands to.
///
/// `Operation.subcommand` is a single **registry** identifier — it has to be, because it is
/// also an `sdim-operation` value and a key the reports partition by. Most identifiers are
/// literally the command word, but `templates-apply` is a two-word command on BOTH sides
/// (`deacon templates apply`, `devcontainer templates apply`), so the identifier is
/// expanded here rather than forcing every case to smuggle `apply` into its `argv`. Doing
/// it in the argv would put half the command name in a field the runner treats as opaque
/// user arguments, and `tokenized_argv` — which records provenance — would then disagree
/// with what was actually run.
fn subcommand_tokens(subcommand: &str) -> Vec<String> {
    match subcommand {
        "templates-apply" => vec!["templates".to_string(), "apply".to_string()],
        other => vec![other.to_string()],
    }
}

/// Attach the failing STAGE and each side's outcome to the `chan-exit-code` verdict's
/// detail (FR-009, and FR-042 for the error-path tier). Path-free and deterministic, so the
/// report stays byte-stable (T018).
///
/// Two things are recorded, and the distinction is the point:
///
/// - **`failurePhase`** — the DECLARED `expectFailurePhase` when the operation carries one,
///   else the coarse inference from the subcommand. The declaration wins because it is the
///   reviewed record and the inference cannot see past the subcommand:
///   [`cli_process::infer_failure_phase`] maps every `run-user-commands` failure to `exec`,
///   so a case pinning `lifecycle:postStart` would have its own recorded stage contradicted
///   by the report it appears in. `inferredFailurePhase` is kept alongside whenever it
///   differs, so nothing is hidden.
/// - **`sides`** — each side's exit code and whether it failed. A differential verdict says
///   only *that* the two disagree; the error-path tier has to record *what each side did*
///   (FR-042), and "deacon exited 1, the reference exited 0" is that fact. Recorded for the
///   reference only when there is a reference run (a `spec-expectation` case has none).
///
/// Attached whenever the operation declares a phase or actually failed — a verdict on an
/// operation that neither declared nor produced a failure has nothing to say here and keeps
/// its detail unchanged.
pub(crate) fn attach_failure_phase(
    verdict: &mut ChannelVerdict,
    case: &TestCase,
    exp: &ExpectedObservable,
    ctx: &RunContext,
    reference: Option<&RunContext>,
) {
    if verdict.channel != CHAN_EXIT_CODE {
        return;
    }
    let Ok(op) = resolve_expected_op(case, exp) else {
        return;
    };
    let outcome = ctx.outcome(&op.id);
    let inferred = outcome.and_then(|o| o.failure_phase);
    let declared = op.expect_failure_phase;
    if declared.is_none() && inferred.is_none() {
        return;
    }

    let mut fields = serde_json::Map::new();
    if let Some(phase) = declared.or(inferred) {
        fields.insert(
            "failurePhase".to_string(),
            serde_json::to_value(phase).unwrap_or(serde_json::Value::Null),
        );
    }
    if let (Some(declared), Some(inferred)) = (declared, inferred)
        && declared != inferred
    {
        fields.insert(
            "inferredFailurePhase".to_string(),
            serde_json::to_value(inferred).unwrap_or(serde_json::Value::Null),
        );
    }
    let mut sides = serde_json::Map::new();
    sides.insert("deacon".to_string(), side_outcome(outcome));
    if let Some(reference) = reference {
        sides.insert(
            "reference".to_string(),
            side_outcome(reference.outcome(&op.id)),
        );
    }
    fields.insert("sides".to_string(), serde_json::Value::Object(sides));

    match verdict.detail.as_mut() {
        Some(serde_json::Value::Object(map)) => map.append(&mut fields),
        _ => verdict.detail = Some(serde_json::Value::Object(fields)),
    }
}

/// How many trailing stderr lines one side's excerpt carries (#474).
const STDERR_EXCERPT_LINES: usize = 20;

/// The byte ceiling on one side's excerpt. Whichever bound bites first wins, so a run that
/// logs one enormous line cannot flood the panic text any more than one that logs a thousand.
const STDERR_EXCERPT_BYTES: usize = 2048;

/// Attach the failing side(s)' captured stderr — tail-bounded — to a DIVERGING
/// `chan-exit-code` verdict (#474).
///
/// An exit-code divergence is the one channel whose verdict names nothing actionable on its
/// own: "the codes disagreed" is the entire message, and the reason the process exited that
/// way is on its stderr. This is formatting over evidence the harness ALREADY captured
/// ([`ProcessOutcome::stderr`], also persisted verbatim under `raw/…`), not new capture.
///
/// Deliberately scoped three ways, because an excerpt everywhere is noise:
///
/// - **`chan-exit-code` only.** A structured-output or container-state divergence already
///   names the diverging path; stderr adds nothing there.
/// - **Diverging verdicts only.** An agreeing exit code — including an agreeing *failure*,
///   which the error-path cases assert — has nothing to explain.
/// - **Non-zero sides only.** A side that succeeded emits progress logs, not a cause.
pub(crate) fn attach_stderr_excerpt(
    verdict: &mut ChannelVerdict,
    case: &TestCase,
    exp: &ExpectedObservable,
    ctx: &RunContext,
    reference: Option<&RunContext>,
) {
    if verdict.channel != CHAN_EXIT_CODE || !matches!(verdict.outcome, Outcome::Diverge) {
        return;
    }
    let Ok(op) = resolve_expected_op(case, exp) else {
        return;
    };
    let mut blocks: Vec<String> = Vec::new();
    if let Some(block) = side_stderr("deacon", ctx.outcome(&op.id)) {
        blocks.push(block);
    }
    if let Some(block) = reference.and_then(|r| side_stderr("reference", r.outcome(&op.id))) {
        blocks.push(block);
    }
    if !blocks.is_empty() {
        verdict.stderr_excerpt = Some(blocks.join("\n"));
    }
}

/// One side's labeled stderr excerpt, or `None` when the side did not run, succeeded, or
/// wrote nothing.
fn side_stderr(label: &str, outcome: Option<&ProcessOutcome>) -> Option<String> {
    let outcome = outcome?;
    if outcome.success {
        return None;
    }
    let excerpt = tail_excerpt(&outcome.stderr)?;
    let code = outcome
        .exit_code
        .map_or_else(|| "signal".to_string(), |c| c.to_string());
    Some(format!("  {label} stderr (exit {code}):\n{excerpt}"))
}

/// The last [`STDERR_EXCERPT_LINES`] lines of `stderr`, further clipped to
/// [`STDERR_EXCERPT_BYTES`], each line prefixed so the excerpt is visibly quoted inside the
/// failure text. `None` when there is nothing to show.
///
/// Bounded from the TAIL because that is where a CLI puts the error it died on. Truncation is
/// announced rather than silent, and the announcement carries only counts derived from the
/// input, so the excerpt's SHAPE is a deterministic function of the stderr it quotes.
fn tail_excerpt(stderr: &[u8]) -> Option<String> {
    // Lossy: a CLI that emitted invalid UTF-8 still has a diagnosable tail, and the raw
    // bytes are preserved on disk regardless.
    let stderr = String::from_utf8_lossy(stderr);
    let trimmed = stderr.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let dropped = lines.len().saturating_sub(STDERR_EXCERPT_LINES);
    let mut tail = lines[dropped..].join("\n");
    let clipped = tail.len() > STDERR_EXCERPT_BYTES;
    if clipped {
        // Walk forward to a char boundary so a multi-byte character is never split.
        let mut start = tail.len() - STDERR_EXCERPT_BYTES;
        while start < tail.len() && !tail.is_char_boundary(start) {
            start += 1;
        }
        tail = tail[start..].to_string();
    }
    let mut out = String::new();
    if dropped > 0 || clipped {
        out.push_str(&format!(
            "    […truncated: showing the last {} line(s), {} byte(s)]\n",
            tail.lines().count(),
            tail.len()
        ));
    }
    for line in tail.lines() {
        out.push_str("    | ");
        out.push_str(line);
        out.push('\n');
    }
    // Trailing newline removed: the caller joins blocks and the driver appends this to a
    // failure line, both of which own their own separators.
    Some(out.trim_end_matches('\n').to_string())
}

/// One side's observable outcome for the exit-code channel: the exit code (`null` for a
/// signal-terminated process, which stays distinct from `0`) and whether it failed.
/// `null` throughout when the operation did not run on that side at all — which is a
/// different claim from "it ran and succeeded".
fn side_outcome(outcome: Option<&ProcessOutcome>) -> serde_json::Value {
    match outcome {
        Some(o) => serde_json::json!({
            "exitCode": o.exit_code,
            "failed": !o.success,
        }),
        None => serde_json::json!({ "exitCode": null, "failed": null }),
    }
}

/// The per-invocation time bound class for a subcommand (config-only vs lifecycle).
fn exec_kind(subcommand: &str) -> ExecKind {
    match subcommand {
        // Config-only: no container is created and no image is pulled, so these finish in
        // the sub-second class. `outdated` and `upgrade` are NOT here — both resolve
        // Feature versions against an OCI registry, which is network-bound.
        "read-configuration" | "doctor" => ExecKind::Config,
        _ => ExecKind::Lifecycle,
    }
}

/// Resolve the workspace an operation runs against. US1 supports a single fixture id
/// mapping to `<fixtures_root>/<id>/`; zero fixtures runs against `fixtures_root`
/// itself. Multiple fixtures per op (merged into one isolated workspace) is US5 —
/// fail-loud until then rather than silently pick one.
/// An operation's `env`, with `${WORKSPACE}` resolved, as the pairs the child is
/// spawned with.
///
/// Only `${WORKSPACE}` is substituted here, and deliberately so. `${IMAGE_TAG}` and
/// `${CONTAINER_ID}` name resources an earlier operation produced and belong in argv,
/// where the case reads as "address THIS container"; resolving them into ambient
/// environment would make the same value reachable by a knob the case never named.
///
/// A value that uses the token with no fixture to root it is the same fail-loud
/// authoring error `argv` reports, for the same reason: the alternative is passing a
/// literal `${WORKSPACE}` to the CLI and testing something nobody wrote.
fn substitute_env(
    case: &TestCase,
    op: &Operation,
    workspace: &Path,
    workspace_is_rooted: bool,
) -> Result<Vec<(String, String)>, HarnessError> {
    let ws = workspace.to_string_lossy();
    let mut out = Vec::with_capacity(op.env.len());
    for (key, value) in &op.env {
        if value.contains(WORKSPACE_TOKEN) && op.fixtures.is_empty() && !workspace_is_rooted {
            return Err(shape_error(
                case,
                &format!(
                    "operation {:?} sets {key} to a value using {WORKSPACE_TOKEN} but declares \
                     no fixture to root it",
                    op.id
                ),
            ));
        }
        out.push((key.clone(), value.replace(WORKSPACE_TOKEN, &ws)));
    }
    Ok(out)
}

/// Substitute `${WORKSPACE}` in an operation's argv with the resolved workspace path. An
/// argv that references the token with no resolvable fixture is a fail-loud authoring
/// error.
fn substitute_argv(
    case: &TestCase,
    op: &Operation,
    workspace: &Path,
    workspace_is_rooted: bool,
    image_tag: Option<&str>,
    container_id: Option<&str>,
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
        // `${IMAGE_TAG}` is only resolvable for a Docker-grouped case, which is what owns
        // the workspace that names and later reclaims the tag. Passing the literal token
        // through to the CLI would create an image nothing reclaims, so this fails loud.
        if arg.contains(IMAGE_TAG_TOKEN) && image_tag.is_none() {
            return Err(shape_error(
                case,
                &format!(
                    "operation {:?} uses {IMAGE_TAG_TOKEN}, which only a Docker-grouped case \
                     can resolve — set `resourceGroup` to a docker group",
                    op.id
                ),
            ));
        }
        // `${CONTAINER_ID}` names a container an EARLIER operation created, so it is only
        // resolvable after one has been observed. Expanding it to nothing would turn a
        // container-addressed invocation into a workspace-addressed one and quietly test
        // the opposite of what the case declares.
        if arg.contains(CONTAINER_ID_TOKEN) && container_id.is_none() {
            return Err(shape_error(
                case,
                &format!(
                    "operation {:?} uses {CONTAINER_ID_TOKEN}, but no earlier operation in this \
                     case has created a container to name",
                    op.id
                ),
            ));
        }
        let mut arg = arg.replace(WORKSPACE_TOKEN, &ws);
        if let Some(tag) = image_tag {
            arg = arg.replace(IMAGE_TAG_TOKEN, tag);
        }
        if let Some(id) = container_id {
            arg = arg.replace(CONTAINER_ID_TOKEN, id);
        }
        out.push(arg);
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
    use crate::model::OracleType;

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

    fn case_with_env(env: &[(&str, &str)], fixtures: &[&str]) -> TestCase {
        let mut case = case_with_op(&["--workspace-folder", "${WORKSPACE}"], fixtures);
        case.operations[0].env = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        case
    }

    #[test]
    fn substitute_env_resolves_the_workspace_token_in_a_value() {
        // The shape that makes SOURCE-shaped knobs reachable: point a variable at a
        // file the case's own fixture materialized.
        let case = case_with_env(
            &[("DEACON_CONTROL_MANIFEST", "${WORKSPACE}/manifest.json")],
            &["fx-x"],
        );
        let out = substitute_env(&case, &case.operations[0], Path::new("/tmp/ws"), false)
            .expect("a fixture roots the token");
        assert_eq!(
            out,
            vec![(
                "DEACON_CONTROL_MANIFEST".to_string(),
                "/tmp/ws/manifest.json".to_string()
            )]
        );
    }

    #[test]
    fn substitute_env_requires_a_fixture_for_the_token() {
        // Same fail-loud rule argv has: passing a literal `${WORKSPACE}` to the CLI
        // would test something nobody wrote.
        let case = case_with_env(&[("DEACON_CACHE_DIR", "${WORKSPACE}/cache")], &[]);
        let err = substitute_env(&case, &case.operations[0], Path::new("/tmp/ws"), false)
            .expect_err("a token with no fixture must fail loud");
        assert!(err.to_string().contains("DEACON_CACHE_DIR"), "{err}");

        // A rooted (isolated Docker) workspace resolves it with no fixture declared.
        let ok = substitute_env(&case, &case.operations[0], Path::new("/tmp/ws"), true)
            .expect("a rooted workspace roots the token");
        assert_eq!(ok[0].1, "/tmp/ws/cache");
    }

    #[test]
    fn substitute_env_orders_pairs_deterministically() {
        // `env` is a BTreeMap, so two authorings that differ only in written order
        // produce the same pairs — and therefore the same `caseHash`.
        let case = case_with_env(&[("B_VAR", "2"), ("A_VAR", "1")], &["fx-x"]);
        let out = substitute_env(&case, &case.operations[0], Path::new("/tmp/ws"), false).unwrap();
        assert_eq!(
            out,
            vec![
                ("A_VAR".to_string(), "1".to_string()),
                ("B_VAR".to_string(), "2".to_string())
            ]
        );
    }

    #[test]
    fn an_operation_without_env_contributes_no_pairs() {
        let case = case_with_op(&["--workspace-folder", "${WORKSPACE}"], &["fx-x"]);
        let out = substitute_env(&case, &case.operations[0], Path::new("/tmp/ws"), false).unwrap();
        assert!(out.is_empty(), "the default must add nothing to the child");
    }

    #[test]
    fn substitute_argv_requires_a_fixture_for_the_token() {
        let case = case_with_op(&["--workspace-folder", "${WORKSPACE}"], &[]);
        // Config-only (not rooted) + no fixture → fail loud.
        let err = substitute_argv(
            &case,
            &case.operations[0],
            Path::new("/tmp/ws"),
            false,
            None,
            None,
        )
        .expect_err("token with no fixture must fail loud");
        assert!(matches!(err, HarnessError::NormalizationFailed { .. }));
        // But a rooted (isolated Docker) workspace resolves the token even with no fixture.
        let ok = substitute_argv(
            &case,
            &case.operations[0],
            Path::new("/tmp/ws"),
            true,
            None,
            None,
        )
        .expect("rooted workspace resolves the token");
        assert_eq!(ok, vec!["--workspace-folder", "/tmp/ws"]);
    }

    #[test]
    fn substitute_argv_resolves_the_container_id_only_after_one_exists() {
        let case = case_with_op(&["--container-id", "${CONTAINER_ID}"], &["fx-x"]);
        let op = &case.operations[0];
        // No container observed yet → fail loud. Expanding the token to nothing would turn
        // a container-addressed invocation into a workspace-addressed one, which is the
        // opposite of what such a case asserts.
        let err = substitute_argv(&case, op, Path::new("/tmp/ws"), false, None, None)
            .expect_err("an unresolvable ${CONTAINER_ID} must fail loud");
        assert!(err.to_string().contains("no earlier operation"), "{err}");

        let ok = substitute_argv(&case, op, Path::new("/tmp/ws"), false, None, Some("abc123"))
            .expect("a container observed by an earlier op resolves the token");
        assert_eq!(ok, vec!["--container-id", "abc123"]);
    }

    #[test]
    fn substitute_argv_replaces_token() {
        let case = case_with_op(&["--workspace-folder", "${WORKSPACE}"], &["fx-x"]);
        let out = substitute_argv(
            &case,
            &case.operations[0],
            Path::new("/tmp/ws"),
            false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["--workspace-folder", "/tmp/ws"]);
    }

    #[test]
    fn substitute_argv_resolves_the_image_tag_only_for_a_docker_case() {
        let case = case_with_op(&["--image-name", "${IMAGE_TAG}"], &["fx-x"]);
        let op = &case.operations[0];
        // No tag available (a non-Docker case) → fail loud rather than pass the literal
        // token through and create an image nothing reclaims.
        let err = substitute_argv(&case, op, Path::new("/tmp/ws"), false, None, None)
            .expect_err("an unresolvable ${IMAGE_TAG} must fail loud");
        assert!(err.to_string().contains("docker group"), "{err}");

        let ok = substitute_argv(
            &case,
            op,
            Path::new("/tmp/ws"),
            false,
            Some("dcr-1-0-img:latest"),
            None,
        )
        .expect("a docker case resolves the tag");
        assert_eq!(ok, vec!["--image-name", "dcr-1-0-img:latest"]);
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

    /// #474: the excerpt is bounded by BOTH rules, and empty stderr produces nothing at all.
    /// The byte ceiling is what keeps one enormous line from flooding the panic text that the
    /// line ceiling alone would wave through.
    #[test]
    fn tail_excerpt_is_bounded_by_lines_and_by_bytes() {
        assert_eq!(tail_excerpt(b""), None, "no stderr, nothing to quote");
        assert_eq!(tail_excerpt(b"   \n\n"), None, "whitespace-only is nothing");

        let short = tail_excerpt(b"boom").expect("non-empty stderr yields an excerpt");
        assert_eq!(
            short, "    | boom",
            "an unbounded excerpt is quoted verbatim"
        );

        // 30 lines → the last 20 survive, and the truncation says so.
        let many: String = (1..=30).map(|i| format!("line-{i:02}\n")).collect();
        let excerpt = tail_excerpt(many.as_bytes()).expect("excerpt");
        assert_eq!(excerpt.lines().count(), 21, "20 quoted lines + the marker");
        assert!(excerpt.contains("line-30") && excerpt.contains("line-11"));
        assert!(!excerpt.contains("line-10"));

        // One line, far over the byte ceiling → clipped to the tail, marker present.
        let huge = "x".repeat(STDERR_EXCERPT_BYTES * 3) + "TAIL";
        let excerpt = tail_excerpt(huge.as_bytes()).expect("excerpt");
        assert!(
            excerpt.len() < STDERR_EXCERPT_BYTES + 200,
            "the byte ceiling must bite even when the line ceiling does not: {} bytes",
            excerpt.len()
        );
        assert!(excerpt.ends_with("TAIL"), "the TAIL is what is kept");
        assert!(excerpt.contains("truncated"));
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
