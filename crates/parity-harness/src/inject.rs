//! Evidence-source-boundary regression injection (024-deterministic-conformance-coverage
//! US6, T134–T136; contracts/regression-harness.md, research Decision 5).
//!
//! This module applies a declarative [`RegressionRecord`] perturbation to the **raw
//! captured artifact** — the completed process result, the pre-fetched `docker inspect`
//! document, or the workspace file bytes — in the gap between capture and observation:
//!
//! ```text
//!         ┌──────────┐   raw artifact    ┌──────────┐   evidence   ┌─────────┐
//!  system ┤ capture  ├──────────────────►│ observer ├─────────────►│ compare │
//!         └──────────┘         ▲         └──────────┘              └─────────┘
//!                              │
//!                     INJECT HERE (legal)      injecting here is FORBIDDEN (FR-065b)
//! ```
//!
//! # Why not one step later (research Decision 5)
//!
//! Perturbing what an observer *returns* would let a **dead** observer — one that ignores
//! its input and always returns the same thing — appear live: the perturbed return value
//! differs, so the channel would report `detected` while observing nothing. Injecting
//! upstream closes that hole. A dead observer ignores the perturbed source, returns its
//! usual value, no difference appears, and the channel is correctly reported `inert`.
//!
//! Two independent mechanisms hold that boundary:
//!
//! 1. **A type-level guard (T135).** Every perturbation entry point is generic over the
//!    SEALED [`EvidenceSource`] trait, which is implemented for
//!    [`RunContext`](crate::observe::RunContext) and nothing else. Its supertrait lives in
//!    a private module, so no type outside this crate — and in particular **not**
//!    [`RawChannelEvidence`](crate::evidence::RawChannelEvidence) or
//!    [`NormalizedChannelEvidence`](crate::evidence::NormalizedChannelEvidence) — can ever
//!    implement it. Injecting downstream of an observer is not a rule to remember; it does
//!    not compile.
//! 2. **A closed target vocabulary.** `EvidenceTarget` (in `deacon-conformance`) names
//!    only raw artifacts, so a record cannot even *ask* for the forbidden point.
//!
//! # Why an ordinary run cannot inject (FR-070)
//!
//! Injection is gated on a process-level capability that only the `coverage-regressions`
//! bin takes out ([`RegressionHarness::declare`]). Until it is declared,
//! [`activate`] refuses with [`HarnessError::InjectionForbidden`] and [`intercept`] — the
//! one hook the runner calls — returns immediately on an unsynchronized atomic load. The
//! ordinary conformance drivers never declare it, so `parity_conformance_runner` and
//! `parity_conformance_docker` cannot apply a regression even by mistake. This is an
//! ENFORCED barrier, not a convention: the guard fails closed, and `injection_faults.rs`
//! asserts both the runtime refusal and the absence of any declaration in the drivers.
//!
//! # Reversibility (FR-066)
//!
//! [`ActiveInjection`] is an RAII guard modelled on
//! [`DockerWorkspace`](crate::workspace::DockerWorkspace): its `Drop` reverts every
//! filesystem change — on success **and** on unwind — and clears the process slot, so a
//! panicking case can never leave a perturbed tree behind. In-memory perturbations need no
//! revert (the `RunContext` is discarded with the case), but they are cleared with the
//! slot all the same so a leaked guard cannot bleed into the next case.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use deacon_conformance::regression::{EvidenceTarget, PerturbationKind, RegressionRecord};
use serde_json::Value;

use crate::HarnessError;
use crate::observe::{ProcessOutcome, RunContext};

// ---------------------------------------------------------------------------
// The capability gate (FR-070)
// ---------------------------------------------------------------------------

/// Set exactly once, by [`RegressionHarness::declare`]. Never cleared: a process either is
/// the regression harness or is not, and "un-declaring" would be a way to smuggle an
/// injection past a later check.
static DECLARED: AtomicBool = AtomicBool::new(false);

/// The process-level capability to inject a regression.
///
/// Constructing one is the ONLY way to enable injection, and only the
/// `coverage-regressions` bin does it. Held as a value rather than a bare function call so
/// the capability is visible in the bin's code as a thing it took out.
#[derive(Debug)]
pub struct RegressionHarness(());

impl RegressionHarness {
    /// Declare this process the regression harness, enabling [`activate`].
    ///
    /// Idempotent, and deliberately irreversible for the life of the process.
    pub fn declare() -> RegressionHarness {
        DECLARED.store(true, Ordering::SeqCst);
        RegressionHarness(())
    }
}

/// Whether this process has declared itself the regression harness.
pub fn is_declared() -> bool {
    DECLARED.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// The sealed evidence-source boundary (T135, FR-065b)
// ---------------------------------------------------------------------------

mod sealed {
    /// Private supertrait: only this module can name it, so [`super::EvidenceSource`]
    /// cannot be implemented anywhere else — including for an observer's return type.
    pub trait Sealed {}
}

/// A RAW captured artifact a perturbation may be applied to — the pre-observer side of
/// the boundary (contract regression-harness.md).
///
/// SEALED. [`RunContext`] is the only implementor, and it is the only thing the runner
/// holds between capture and observation. There is deliberately no implementation for
/// `RawChannelEvidence`: an observer's output is downstream of the boundary, and a dead
/// observer must be reported `inert` rather than falsely `detected` (research Decision 5).
pub trait EvidenceSource: sealed::Sealed {
    /// The workspace the case ran in — the root a `workspace-file` perturbation resolves
    /// against.
    fn workspace_path(&self) -> &Path;

    /// Visit every captured process result, in a deterministic order.
    fn visit_outcomes(&mut self, visit: &mut dyn FnMut(&mut ProcessOutcome));

    /// The pre-fetched `docker inspect` document, when the case brought up a container.
    fn inspect_document_mut(&mut self) -> Option<&mut Value>;
}

impl sealed::Sealed for RunContext {}

impl EvidenceSource for RunContext {
    fn workspace_path(&self) -> &Path {
        &self.workspace
    }

    fn visit_outcomes(&mut self, visit: &mut dyn FnMut(&mut ProcessOutcome)) {
        for outcome in self.outcomes_mut() {
            visit(outcome);
        }
    }

    fn inspect_document_mut(&mut self) -> Option<&mut Value> {
        self.container_inspect.as_mut()
    }
}

// ---------------------------------------------------------------------------
// The active-injection slot + RAII guard (T136)
// ---------------------------------------------------------------------------

/// The one process-wide slot. At most one regression is active at a time: a run that
/// applied two perturbations at once could not attribute a failure to either.
fn slot() -> &'static Mutex<Option<InjectionState>> {
    static SLOT: OnceLock<Mutex<Option<InjectionState>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// What the slot holds while an injection is active.
struct InjectionState {
    record: RegressionRecord,
    /// How many evidence sources the perturbation was applied to. Zero at the end of a
    /// run means the perturbation never landed — which must be reported as a harness
    /// fault, NOT as `inert`, because it says nothing about the channel.
    applied: usize,
    /// Filesystem changes to undo, newest first.
    reverts: Vec<Revert>,
}

/// One reversible filesystem change.
#[derive(Debug)]
struct Revert {
    path: PathBuf,
    /// The file's bytes before the perturbation, or `None` when it did not exist (revert
    /// then removes whatever the perturbation created).
    before: Option<Vec<u8>>,
}

/// The RAII guard for an active injection. Dropping it reverts every filesystem change
/// and frees the slot — on success and on unwind alike (FR-066), mirroring
/// [`DockerWorkspace`](crate::workspace::DockerWorkspace).
#[derive(Debug)]
pub struct ActiveInjection {
    id: String,
    /// Set once [`ActiveInjection::finish`] has run, so `Drop` does not revert twice.
    released: bool,
    /// Reverts that failed, surfaced by [`ActiveInjection::finish`] (a perturbation that
    /// cannot be reverted is exit-1 per the contract, never a warning).
    failures: Vec<String>,
}

impl ActiveInjection {
    /// How many evidence sources this injection was applied to so far.
    pub fn applied_count(&self) -> usize {
        slot()
            .lock()
            .map(|guard| guard.as_ref().map_or(0, |s| s.applied))
            .unwrap_or(0)
    }

    /// Revert now and report any failure, instead of leaving it to `Drop` (which cannot
    /// return one). Idempotent.
    pub fn finish(mut self) -> Result<(), HarnessError> {
        self.release();
        if self.failures.is_empty() {
            return Ok(());
        }
        Err(HarnessError::InjectionRevertFailed {
            record: self.id.clone(),
            cause: self.failures.join("; "),
        })
    }

    /// Revert the filesystem and clear the slot. Never panics (it runs from `Drop`).
    fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let taken = match slot().lock() {
            Ok(mut guard) => guard.take(),
            // A poisoned slot means another thread panicked holding it; recover the state
            // anyway rather than leaking a perturbed file.
            Err(poisoned) => poisoned.into_inner().take(),
        };
        let Some(state) = taken else {
            return;
        };
        for revert in state.reverts.into_iter().rev() {
            if let Err(cause) = apply_revert(&revert) {
                self.failures.push(cause);
            }
        }
    }
}

impl Drop for ActiveInjection {
    fn drop(&mut self) {
        // RAII guarantee: revert on success AND on unwind (panic / early return). Failures
        // are already surfaced by `finish`; on the unwind path there is nobody to return
        // them to, so they are reported to stderr rather than swallowed or re-panicked
        // (a panic in `Drop` during unwind aborts the process).
        self.release();
        for failure in &self.failures {
            eprintln!(
                "error: injected regression `{}` could not be reverted: {failure}",
                self.id
            );
        }
    }
}

/// Undo one filesystem change: restore the saved bytes, or remove the file when there
/// were none (it did not exist before).
///
/// A perturbed file whose containing DIRECTORY has since disappeared needs no revert and
/// is not a failure: that is the ordinary end of a Docker / `fs-heavy` case, whose isolated
/// temp workspace is reclaimed by [`DockerWorkspace`](crate::workspace::DockerWorkspace)'s
/// guard as soon as observation finishes — before this guard unwinds. Nothing can be left
/// perturbed when the whole tree is gone. The check is on the PARENT rather than on a
/// `NotFound` from the write, because a missing parent is the only shape that means "the
/// tree went away"; a `NotFound` on the file itself, with its directory still present,
/// would be a genuine restore failure and must still be reported.
fn apply_revert(revert: &Revert) -> Result<(), String> {
    if revert.path.parent().is_some_and(|p| !p.exists()) {
        return Ok(());
    }
    match &revert.before {
        Some(bytes) => std::fs::write(&revert.path, bytes)
            .map_err(|e| format!("could not restore {:?}: {e}", revert.path)),
        None => match std::fs::remove_file(&revert.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("could not remove {:?}: {e}", revert.path)),
        },
    }
}

/// Arm `record` for the next run, returning the RAII guard that disarms and reverts it.
///
/// Fails loud when the process has not declared itself the regression harness (FR-070) or
/// when an injection is already active — two simultaneous perturbations could not be
/// attributed to either channel.
pub fn activate(record: &RegressionRecord) -> Result<ActiveInjection, HarnessError> {
    if !is_declared() {
        return Err(HarnessError::InjectionForbidden {
            record: record.id.clone(),
        });
    }
    let mut guard = slot()
        .lock()
        .map_err(|_| HarnessError::InjectionForbidden {
            record: record.id.clone(),
        })?;
    if let Some(active) = guard.as_ref() {
        return Err(HarnessError::InjectionInapplicable {
            record: record.id.clone(),
            cause: format!(
                "regression `{}` is already active; only one perturbation may be applied at a \
                 time, or a failure could not be attributed to either",
                active.record.id
            ),
        });
    }
    *guard = Some(InjectionState {
        record: record.clone(),
        applied: 0,
        reverts: Vec::new(),
    });
    Ok(ActiveInjection {
        id: record.id.clone(),
        released: false,
        failures: Vec::new(),
    })
}

/// How many times the runner has consulted the injector in this process — the signal the
/// FR-070 guard test reads to prove an ORDINARY run reaches the hook and is refused by it,
/// rather than the hook simply not being wired.
static INTERCEPTS: AtomicUsize = AtomicUsize::new(0);

/// How many times [`intercept`] has been called in this process.
pub fn intercept_count() -> usize {
    INTERCEPTS.load(Ordering::SeqCst)
}

/// The ONE hook the runner calls, between capturing a case's evidence and handing it to
/// the observers.
///
/// A no-op — one relaxed atomic load — unless this process declared itself the regression
/// harness AND a regression is armed. Only deacon's side is perturbed: perturbing both
/// sides of a live differential identically would leave them agreeing, which is exactly
/// the difference the record exists to surface.
pub fn intercept(ctx: &mut RunContext) -> Result<(), HarnessError> {
    INTERCEPTS.fetch_add(1, Ordering::SeqCst);
    if !is_declared() {
        return Ok(());
    }
    if ctx.side != crate::exec::Side::Deacon {
        return Ok(());
    }
    let mut guard = slot()
        .lock()
        .map_err(|_| HarnessError::InjectionForbidden {
            record: "<poisoned>".to_string(),
        })?;
    let Some(state) = guard.as_mut() else {
        return Ok(());
    };
    let record = state.record.clone();
    let outcome = perturb(ctx, &record)?;
    state.applied += outcome.applied;
    state.reverts.extend(outcome.reverts);
    Ok(())
}

/// What one application of a perturbation did.
#[derive(Debug)]
struct Applied {
    /// How many distinct artifacts were perturbed (zero ⇒ the perturbation did not land).
    applied: usize,
    /// Filesystem changes to undo.
    reverts: Vec<Revert>,
}

/// Apply `record`'s perturbation to `source`, the RAW captured artifact.
///
/// Generic over the SEALED [`EvidenceSource`] (T135): the signature makes injecting into
/// an observer's returned evidence a type error, not a rule to remember.
///
/// Every failure to apply is a fail-loud [`HarnessError::InjectionInapplicable`]. That
/// distinction is load-bearing: a perturbation that never landed must never be reported as
/// `inert`, because `inert` is a claim about the CHANNEL and this is a claim about the
/// record.
fn perturb<S: EvidenceSource + ?Sized>(
    source: &mut S,
    record: &RegressionRecord,
) -> Result<Applied, HarnessError> {
    let p = &record.perturbation;
    let inapplicable = |cause: String| HarnessError::InjectionInapplicable {
        record: record.id.clone(),
        cause,
    };

    match record.target {
        EvidenceTarget::ProcessResult => {
            let exit_code = p
                .exit_code
                .ok_or_else(|| inapplicable("`set-exit-code` carries no `exitCode`".to_string()))?;
            let mut applied = 0usize;
            source.visit_outcomes(&mut |outcome| {
                outcome.exit_code = Some(exit_code);
                outcome.success = exit_code == 0;
                applied += 1;
            });
            require_applied(applied, "no operation result was captured", &inapplicable)
        }
        EvidenceTarget::ProcessStdout | EvidenceTarget::ProcessStderr => {
            let bytes = p
                .bytes
                .clone()
                .ok_or_else(|| inapplicable("`append-bytes` carries no `bytes`".to_string()))?;
            let stdout = record.target == EvidenceTarget::ProcessStdout;
            let mut applied = 0usize;
            source.visit_outcomes(&mut |outcome| {
                let stream = if stdout {
                    &mut outcome.stdout
                } else {
                    &mut outcome.stderr
                };
                stream.extend_from_slice(bytes.as_bytes());
                applied += 1;
            });
            require_applied(applied, "no operation result was captured", &inapplicable)
        }
        EvidenceTarget::StructuredOutputDocument => {
            // Perturbed AT ITS SOURCE: the stdout bytes are re-parsed as JSON, the pointer
            // operation applied, and the document written back. The structured observer
            // then parses the perturbed bytes exactly as it parses real output — which is
            // what keeps the injection upstream of the observer.
            let mut applied = 0usize;
            let mut failure: Option<String> = None;
            source.visit_outcomes(&mut |outcome| {
                let text = String::from_utf8_lossy(&outcome.stdout).into_owned();
                let Ok(mut doc) = serde_json::from_str::<Value>(text.trim()) else {
                    return; // not a structured-output operation; nothing to perturb here
                };
                match apply_pointer(&mut doc, record) {
                    Ok(()) => {
                        match serde_json::to_vec(&doc) {
                            Ok(bytes) => {
                                outcome.stdout = bytes;
                                applied += 1;
                            }
                            Err(e) => failure = Some(format!("could not re-serialize: {e}")),
                        };
                    }
                    Err(cause) => failure = Some(cause),
                }
            });
            if let Some(cause) = failure {
                return Err(inapplicable(cause));
            }
            require_applied(
                applied,
                "no operation produced a JSON document on stdout",
                &inapplicable,
            )
        }
        EvidenceTarget::ContainerInspectDocument | EvidenceTarget::ImageInspectDocument => {
            let doc = source.inspect_document_mut().ok_or_else(|| {
                inapplicable(
                    "the case captured no `docker inspect` document (no container was \
                     created, or it was removed before observation)"
                        .to_string(),
                )
            })?;
            apply_pointer(doc, record).map_err(inapplicable)?;
            Ok(Applied {
                applied: 1,
                reverts: Vec::new(),
            })
        }
        EvidenceTarget::WorkspaceFile => {
            let rel = p
                .path
                .as_deref()
                .ok_or_else(|| inapplicable("carries no `path`".to_string()))?;
            let abs = source.workspace_path().join(rel);
            let before = std::fs::read(&abs).map_err(|e| {
                inapplicable(format!(
                    "could not read the workspace file {rel:?} the perturbation targets: {e}"
                ))
            })?;
            let revert = Revert {
                path: abs.clone(),
                before: Some(before.clone()),
            };
            match p.kind {
                PerturbationKind::RemovePath => std::fs::remove_file(&abs)
                    .map_err(|e| inapplicable(format!("could not remove {rel:?}: {e}")))?,
                PerturbationKind::AppendBytes => {
                    let marker = p.bytes.as_deref().ok_or_else(|| {
                        inapplicable("`append-bytes` carries no `bytes`".to_string())
                    })?;
                    let mut next = before;
                    next.extend_from_slice(marker.as_bytes());
                    std::fs::write(&abs, &next)
                        .map_err(|e| inapplicable(format!("could not rewrite {rel:?}: {e}")))?;
                }
                other => {
                    return Err(inapplicable(format!(
                        "perturbation kind `{}` does not apply to a workspace file",
                        other.as_str()
                    )));
                }
            }
            Ok(Applied {
                applied: 1,
                reverts: vec![revert],
            })
        }
    }
}

/// Apply a perturbation to `source`, gated on the FR-070 capability.
///
/// The public entry point the fault tests drive. Ordinary code never calls it; the runner
/// goes through [`intercept`], which is a no-op without the capability.
pub fn perturb_source<S: EvidenceSource + ?Sized>(
    source: &mut S,
    record: &RegressionRecord,
) -> Result<usize, HarnessError> {
    if !is_declared() {
        return Err(HarnessError::InjectionForbidden {
            record: record.id.clone(),
        });
    }
    // Filesystem reverts are dropped here on purpose: this entry point is for perturbing
    // an in-memory source. A `workspace-file` record must go through `activate` +
    // `intercept`, whose guard owns the revert.
    let applied = perturb(source, record)?;
    if !applied.reverts.is_empty() {
        for revert in &applied.reverts {
            let _ = apply_revert(revert);
        }
        return Err(HarnessError::InjectionInapplicable {
            record: record.id.clone(),
            cause: "a `workspace-file` perturbation must be armed with `activate` so its \
                    revert is owned by the RAII guard"
                .to_string(),
        });
    }
    Ok(applied.applied)
}

/// Turn a zero application count into a fail-loud error (never a silent no-op).
fn require_applied(
    applied: usize,
    cause: &str,
    inapplicable: &dyn Fn(String) -> HarnessError,
) -> Result<Applied, HarnessError> {
    if applied == 0 {
        return Err(inapplicable(format!(
            "the perturbation landed on nothing: {cause}. A perturbation that was never \
             applied says nothing about the channel, so it is a harness fault rather than \
             an `inert` verdict"
        )));
    }
    Ok(Applied {
        applied,
        reverts: Vec::new(),
    })
}

/// Apply a JSON-pointer perturbation to `doc`.
///
/// `set-json-pointer` requires the pointer's PARENT to exist (setting through a missing
/// parent would invent structure the real document never had); `remove-json-pointer`
/// requires the pointer itself to resolve, since removing something absent perturbs
/// nothing.
fn apply_pointer(doc: &mut Value, record: &RegressionRecord) -> Result<(), String> {
    let p = &record.perturbation;
    let pointer = p
        .pointer
        .as_deref()
        .ok_or_else(|| "the perturbation carries no `pointer`".to_string())?;
    let (parent, leaf) = pointer
        .rsplit_once('/')
        .ok_or_else(|| format!("pointer {pointer:?} has no parent segment"))?;
    let leaf = unescape_pointer_token(leaf);

    match p.kind {
        PerturbationKind::SetJsonPointer => {
            let value = p
                .value
                .clone()
                .ok_or_else(|| "`set-json-pointer` carries no `value`".to_string())?;
            let target = doc
                .pointer_mut(parent)
                .ok_or_else(|| format!("pointer parent {parent:?} does not resolve"))?;
            match target {
                Value::Object(map) => {
                    map.insert(leaf, value);
                    Ok(())
                }
                Value::Array(items) => {
                    let index: usize = leaf
                        .parse()
                        .map_err(|_| format!("array index {leaf:?} is not a number"))?;
                    let slot = items
                        .get_mut(index)
                        .ok_or_else(|| format!("array index {index} is out of range"))?;
                    *slot = value;
                    Ok(())
                }
                _ => Err(format!(
                    "pointer parent {parent:?} is neither an object nor an array"
                )),
            }
        }
        PerturbationKind::RemoveJsonPointer => {
            let target = doc
                .pointer_mut(parent)
                .ok_or_else(|| format!("pointer parent {parent:?} does not resolve"))?;
            match target {
                Value::Object(map) => map.remove(&leaf).map(|_| ()).ok_or_else(|| {
                    format!(
                        "pointer {pointer:?} does not resolve, so removing it would perturb \
                         nothing"
                    )
                }),
                Value::Array(items) => {
                    let index: usize = leaf
                        .parse()
                        .map_err(|_| format!("array index {leaf:?} is not a number"))?;
                    if index >= items.len() {
                        return Err(format!("array index {index} is out of range"));
                    }
                    items.remove(index);
                    Ok(())
                }
                _ => Err(format!(
                    "pointer parent {parent:?} is neither an object nor an array"
                )),
            }
        }
        other => Err(format!(
            "perturbation kind `{}` is not a JSON-pointer operation",
            other.as_str()
        )),
    }
}

/// RFC-6901 token unescaping: `~1` → `/`, `~0` → `~` (in that order).
fn unescape_pointer_token(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

// ---------------------------------------------------------------------------
// Verdicts + the run report (contract regression-harness.md, "Report")
// ---------------------------------------------------------------------------

/// A record's / channel's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegressionVerdict {
    /// ≥1 case failed, and the failure is attributed to the record's channel.
    Detected,
    /// Every record for the channel went undetected.
    Inert,
}

/// Classify ONE (record, case) pair from the case's channel outcome before and after the
/// perturbation.
///
/// Attribution is the whole point (contract regression-harness.md, "Verdicts"): a case
/// that was ALREADY failing on this channel proves nothing, because its post-injection
/// failure is not evidence the injection caused anything. So detection requires the
/// baseline to be clean on that channel and the perturbed run not to be.
pub fn detects(
    baseline: Option<crate::evidence::Outcome>,
    perturbed: Option<crate::evidence::Outcome>,
) -> bool {
    use crate::evidence::Outcome;
    let clean = |o: Option<Outcome>| matches!(o, Some(Outcome::Agree | Outcome::AllowedDifference));
    let failed =
        |o: Option<Outcome>| matches!(o, Some(Outcome::Diverge | Outcome::Stale | Outcome::Error));
    clean(baseline) && failed(perturbed)
}

/// One record's outcome in a run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordResult {
    /// The `reg-` id.
    pub id: String,
    /// The cases that ACTUALLY detected it (may be a strict subset of the record's
    /// declared candidates — the run reports what happened, not what was expected).
    pub detected_by: Vec<String>,
    /// Non-blocking observations: a candidate whose baseline was not clean, or which does
    /// not observe the channel. Surfaced so an undetected record is never mysterious.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// One channel's roll-up.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReport {
    /// The `chan-` id.
    pub channel: String,
    /// `detected` when ≥1 of its records was detected.
    pub verdict: RegressionVerdict,
    /// Its records, id-sorted.
    pub records: Vec<RecordResult>,
}

/// The `target/conformance/regressions.json` document (contract regression-harness.md).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegressionReport {
    /// Report schema version.
    pub schema_version: u32,
    /// One entry per channel exercised, channel-id-sorted.
    pub channels: Vec<ChannelReport>,
    /// The number SC-006 requires to be zero.
    pub inert_count: usize,
}

impl RegressionReport {
    /// Roll per-record results up into the per-channel report.
    ///
    /// `results` is `(channel, RecordResult)`; ordering of the input does not matter — the
    /// output is channel-id-sorted then record-id-sorted, so the document is byte-stable.
    pub fn build(results: Vec<(String, RecordResult)>) -> RegressionReport {
        let mut by_channel: BTreeMap<String, Vec<RecordResult>> = BTreeMap::new();
        for (channel, result) in results {
            by_channel.entry(channel).or_default().push(result);
        }
        let channels: Vec<ChannelReport> = by_channel
            .into_iter()
            .map(|(channel, mut records)| {
                records.sort_by(|a, b| a.id.cmp(&b.id));
                let verdict = if records.iter().any(|r| !r.detected_by.is_empty()) {
                    RegressionVerdict::Detected
                } else {
                    RegressionVerdict::Inert
                };
                ChannelReport {
                    channel,
                    verdict,
                    records,
                }
            })
            .collect();
        let inert_count = channels
            .iter()
            .filter(|c| c.verdict == RegressionVerdict::Inert)
            .count();
        RegressionReport {
            schema_version: 1,
            channels,
            inert_count,
        }
    }

    /// Byte-stable pretty JSON with a trailing newline.
    pub fn render(&self) -> Result<String, HarnessError> {
        let mut s = serde_json::to_string_pretty(self).map_err(|e| HarnessError::Report {
            cause: format!("could not serialize the regression report: {e}"),
        })?;
        s.push('\n');
        Ok(s)
    }

    /// The run's exit status per contract regression-harness.md: `0` when every exercised
    /// channel has ≥1 detected record, `1` when any is inert.
    ///
    /// A function rather than a rule the bin restates, so the test that asserts "an inert
    /// channel FAILS the run" (FR-067) is asserting the same decision the bin makes.
    pub fn exit_status(&self) -> u8 {
        if self.inert_count == 0 { 0 } else { 1 }
    }

    /// The inert channels, id-sorted — the failure list.
    pub fn inert_channels(&self) -> Vec<&str> {
        self.channels
            .iter()
            .filter(|c| c.verdict == RegressionVerdict::Inert)
            .map(|c| c.channel.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_conformance::regression::RegressionFile;

    fn record(json: &str) -> RegressionRecord {
        let file: RegressionFile =
            serde_json::from_str(&format!(r#"{{"records":[{json}]}}"#)).expect("record loads");
        file.records.into_iter().next().expect("one record")
    }

    fn exit_code_record() -> RegressionRecord {
        record(
            r#"{
              "id": "reg-x",
              "channel": "chan-exit-code",
              "target": "process-result",
              "perturbation": { "kind": "set-exit-code", "exitCode": 0 },
              "expectedDetectingCases": ["case-x"]
            }"#,
        )
    }

    fn ctx_with_outcome(stdout: &str, exit: i32) -> RunContext {
        let mut ctx = RunContext::new(PathBuf::from("/tmp"));
        ctx.record_outcome(
            "op",
            ProcessOutcome {
                exit_code: Some(exit),
                success: exit == 0,
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
                failure_phase: None,
            },
        );
        ctx
    }

    /// The FR-070 gate fails CLOSED: without the capability nothing can be armed or
    /// applied, and the diagnosis names the record.
    #[test]
    fn without_the_capability_nothing_can_be_injected() {
        // NOTE: this test must not call `RegressionHarness::declare` — the capability is
        // process-wide and irreversible. The tests that need it live in
        // `tests/injection_faults.rs`, which is a separate process.
        if is_declared() {
            return; // another test in this binary declared it; the guard is covered there
        }
        let rec = exit_code_record();
        assert!(matches!(
            activate(&rec),
            Err(HarnessError::InjectionForbidden { .. })
        ));
        let mut ctx = ctx_with_outcome("", 1);
        assert!(matches!(
            perturb_source(&mut ctx, &rec),
            Err(HarnessError::InjectionForbidden { .. })
        ));
    }

    /// `perturb` itself (the capability-free internal) applies each kind to the RAW
    /// artifact. Driven directly so the pure mapping is covered without the process-wide
    /// capability.
    #[test]
    fn set_exit_code_rewrites_every_captured_result() {
        let mut ctx = ctx_with_outcome("", 1);
        let applied = perturb(&mut ctx, &exit_code_record()).expect("applies");
        assert_eq!(applied.applied, 1);
        let outcome = ctx.outcome("op").expect("outcome");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.success, "success tracks the perturbed status");
    }

    #[test]
    fn append_bytes_extends_the_captured_stream() {
        let rec = record(
            r#"{
              "id": "reg-y",
              "channel": "chan-stdout",
              "target": "process-stdout",
              "perturbation": { "kind": "append-bytes", "bytes": "INJECTED" },
              "expectedDetectingCases": ["case-x"]
            }"#,
        );
        let mut ctx = ctx_with_outcome("hello", 0);
        perturb(&mut ctx, &rec).expect("applies");
        assert_eq!(
            String::from_utf8_lossy(&ctx.outcome("op").unwrap().stdout),
            "helloINJECTED"
        );
    }

    #[test]
    fn structured_output_is_perturbed_at_its_source_bytes() {
        let rec = record(
            r#"{
              "id": "reg-z",
              "channel": "chan-structured-output",
              "target": "structured-output-document",
              "perturbation": { "kind": "remove-json-pointer", "pointer": "/configuration/name" },
              "expectedDetectingCases": ["case-x"]
            }"#,
        );
        let mut ctx = ctx_with_outcome(r#"{"configuration":{"name":"x","image":"alpine"}}"#, 0);
        perturb(&mut ctx, &rec).expect("applies");
        // The STDOUT BYTES changed — the observer will parse the perturbed document, which
        // is what keeps the injection upstream of it.
        let text = String::from_utf8_lossy(&ctx.outcome("op").unwrap().stdout).into_owned();
        let doc: Value = serde_json::from_str(&text).expect("still JSON");
        assert!(doc["configuration"].get("name").is_none());
        assert_eq!(doc["configuration"]["image"], "alpine");
    }

    #[test]
    fn removing_an_absent_pointer_is_a_fail_loud_fault_not_a_silent_no_op() {
        let rec = record(
            r#"{
              "id": "reg-z",
              "channel": "chan-temporal",
              "target": "container-inspect-document",
              "perturbation": { "kind": "remove-json-pointer", "pointer": "/State/Nope" },
              "expectedDetectingCases": ["case-x"]
            }"#,
        );
        let mut ctx = RunContext::new(PathBuf::from("/tmp"));
        ctx.container_inspect = Some(serde_json::json!({ "State": { "Status": "running" } }));
        let err = perturb(&mut ctx, &rec).expect_err("an absent pointer perturbs nothing");
        assert!(matches!(err, HarnessError::InjectionInapplicable { .. }));
    }

    #[test]
    fn a_case_with_no_container_reports_an_inapplicable_fault() {
        let rec = record(
            r#"{
              "id": "reg-z",
              "channel": "chan-image",
              "target": "image-inspect-document",
              "perturbation": { "kind": "set-json-pointer", "pointer": "/Config/Labels/a", "value": "b" },
              "expectedDetectingCases": ["case-x"]
            }"#,
        );
        let mut ctx = RunContext::new(PathBuf::from("/tmp"));
        let err = perturb(&mut ctx, &rec).expect_err("no inspect document");
        assert!(matches!(err, HarnessError::InjectionInapplicable { .. }));
    }

    #[test]
    fn set_json_pointer_writes_through_an_existing_parent_only() {
        let rec = record(
            r#"{
              "id": "reg-z",
              "channel": "chan-image",
              "target": "image-inspect-document",
              "perturbation": { "kind": "set-json-pointer", "pointer": "/Config/Labels/x", "value": "injected" },
              "expectedDetectingCases": ["case-x"]
            }"#,
        );
        let mut ok = RunContext::new(PathBuf::from("/tmp"));
        ok.container_inspect = Some(serde_json::json!({ "Config": { "Labels": { "x": "real" } } }));
        perturb(&mut ok, &rec).expect("applies");
        assert_eq!(
            ok.container_inspect.as_ref().unwrap()["Config"]["Labels"]["x"],
            "injected"
        );

        // A missing parent is refused rather than invented — a perturbation that creates
        // structure the real document never had is not a regression of anything.
        let mut missing = RunContext::new(PathBuf::from("/tmp"));
        missing.container_inspect = Some(serde_json::json!({ "Config": {} }));
        assert!(perturb(&mut missing, &rec).is_err());
    }

    #[test]
    fn pointer_tokens_are_rfc6901_unescaped() {
        assert_eq!(
            unescape_pointer_token("devcontainer.source"),
            "devcontainer.source"
        );
        assert_eq!(unescape_pointer_token("a~1b"), "a/b");
        assert_eq!(unescape_pointer_token("a~0b"), "a~b");
    }

    #[test]
    fn detection_requires_a_clean_baseline_and_a_failing_perturbed_run() {
        use crate::evidence::Outcome;
        assert!(detects(Some(Outcome::Agree), Some(Outcome::Diverge)));
        assert!(detects(
            Some(Outcome::AllowedDifference),
            Some(Outcome::Error)
        ));
        // An ALREADY-failing case proves nothing: its failure is not caused by us.
        assert!(!detects(Some(Outcome::Diverge), Some(Outcome::Diverge)));
        // A perturbed run that still agrees is no detection.
        assert!(!detects(Some(Outcome::Agree), Some(Outcome::Agree)));
        // A channel the case never verdicted cannot detect anything.
        assert!(!detects(Some(Outcome::Agree), None));
        assert!(!detects(None, Some(Outcome::Diverge)));
    }

    #[test]
    fn the_report_is_channel_sorted_and_counts_inert_channels() {
        let report = RegressionReport::build(vec![
            (
                "chan-stdout".to_string(),
                RecordResult {
                    id: "reg-b".to_string(),
                    detected_by: vec![],
                    notes: vec![],
                },
            ),
            (
                "chan-exit-code".to_string(),
                RecordResult {
                    id: "reg-a".to_string(),
                    detected_by: vec!["case-x".to_string()],
                    notes: vec![],
                },
            ),
        ]);
        assert_eq!(report.schema_version, 1);
        assert_eq!(
            report
                .channels
                .iter()
                .map(|c| c.channel.as_str())
                .collect::<Vec<_>>(),
            vec!["chan-exit-code", "chan-stdout"],
            "channels are id-sorted so the document is byte-stable"
        );
        assert_eq!(report.channels[0].verdict, RegressionVerdict::Detected);
        assert_eq!(report.channels[1].verdict, RegressionVerdict::Inert);
        assert_eq!(report.inert_count, 1);
        assert_eq!(report.inert_channels(), vec!["chan-stdout"]);
        let rendered = report.render().expect("renders");
        assert!(rendered.ends_with('\n'));
        assert_eq!(
            rendered,
            RegressionReport::build(vec![
                (
                    "chan-exit-code".to_string(),
                    RecordResult {
                        id: "reg-a".to_string(),
                        detected_by: vec!["case-x".to_string()],
                        notes: vec![],
                    },
                ),
                (
                    "chan-stdout".to_string(),
                    RecordResult {
                        id: "reg-b".to_string(),
                        detected_by: vec![],
                        notes: vec![],
                    },
                ),
            ])
            .render()
            .expect("renders"),
            "the same results in a different order render identically"
        );
    }
}
