//! BuildKit `--progress=plain` output parser (Part 1 / B1).
//!
//! A pure, stateful model that consumes BuildKit's `--progress=plain` output
//! one line at a time via [`BuildProgress::push_line`] and exposes a structured
//! view of the build steps. It is deliberately free of any rendering or IO so it
//! can be unit-tested against captured fixtures without Docker; the compact
//! `MultiProgress` renderer (B3) drives it and repaints from [`BuildProgress::steps`].
//!
//! # Grammar
//!
//! BuildKit prefixes every step line with `#<N>` where `N` is the step number:
//!
//! - `#N [<stage> <k>/<M>] <op…>` — step header (first sighting of `#N`), e.g.
//!   `#8 [stage-0 2/3] RUN --mount=…`. The bracket may also be `[internal]` or
//!   `[context …]`, and some steps (image export) have no bracket at all.
//! - `#N <elapsed> <msg>` — a line of the step's output, e.g. `#8 0.133 Installing…`.
//! - `#N DONE <t>s` — the step finished successfully.
//! - `#N CACHED` — the step was served from cache.
//! - `#N ERROR: …` — the step failed (carries the exit code).
//!
//! On failure BuildKit also prints a trailing `------` / `Dockerfile:NN` block
//! re-echoing the failing step. We do not need it: the failing step's own `#N`
//! output lines (including its `ERROR:` line) are captured during streaming and
//! surfaced by [`BuildProgress::failing_step_log`].
//!
//! # Feature steps
//!
//! deacon's generated feature-install `RUN` carries a BuildKit bind mount
//! `--mount=…,source=<sanitized_id>_<level>,…` (see
//! `DockerfileGenerator::generate_feature_install_command`). We recover
//! `<sanitized_id>` from that marker and, when the caller has registered the
//! original feature ids via [`BuildProgress::register_feature`], map it back to
//! the friendly id — reusing `DockerfileGenerator::sanitize_feature_id` so the
//! two never drift.
//!
//! B1 lands this parser (with fixtures + unit tests) ahead of the B3
//! `MultiProgress` renderer that consumes it. Until that wiring exists, the
//! `deacon` *binary* target compiles this module (via `main.rs`'s `mod ui`)
//! without calling into it, so the whole surface reads as dead code there — the
//! library target and unit tests exercise it fully. Remove this allow when B3
//! wires the renderer to `BuildProgress`.
#![allow(dead_code)]

use std::collections::HashMap;

use deacon_core::dockerfile_generator::DockerfileGenerator;

/// Marker BuildKit emits for each feature-install bind mount, e.g.
/// `--mount=type=bind,…,source=node_0,target=…`.
const SOURCE_MARKER: &str = ",source=";

/// Cap on the number of trailing lines returned by [`BuildProgress::failing_step_log`].
const FAILING_LOG_TAIL_LINES: usize = 50;

/// Lifecycle status of a single build step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// Header seen, no terminal marker yet.
    Running,
    /// `#N DONE <t>s`.
    Done,
    /// `#N CACHED`.
    Cached,
    /// `#N ERROR: …`.
    Error,
}

/// A single BuildKit step (`#N`).
#[derive(Debug, Clone)]
pub struct BuildStep {
    /// BuildKit step number (`#N`).
    pub id: u32,
    /// Display label: the friendly feature id for feature steps, otherwise the
    /// step's operation text.
    pub label: String,
    /// Full operation text from the header line (after the `[…]` bracket).
    pub op: String,
    /// Leading token of the `[…]` bracket, e.g. `stage-0`, `internal`, `context`.
    pub stage: Option<String>,
    /// Current status.
    pub status: StepStatus,
    /// Elapsed seconds parsed from the `DONE <t>s` marker, when present.
    pub elapsed_secs: Option<f64>,
    /// Recovered friendly feature id when this step installs a feature.
    pub feature: Option<String>,
    /// Captured output lines (`#N …`), elapsed prefix stripped, incl. the
    /// `ERROR:` line on failure. Used for [`BuildProgress::failing_step_log`].
    log: Vec<String>,
}

impl BuildStep {
    /// Whether this step is a deacon feature-install step.
    pub fn is_feature(&self) -> bool {
        self.feature.is_some()
    }

    /// The step's captured output lines.
    pub fn log_lines(&self) -> &[String] {
        &self.log
    }
}

/// Stateful parser for BuildKit `--progress=plain` output.
#[derive(Debug, Default)]
pub struct BuildProgress {
    steps: Vec<BuildStep>,
    index: HashMap<u32, usize>,
    /// `sanitized_id -> original feature id`, populated via [`Self::register_feature`].
    feature_names: HashMap<String, String>,
    failing_id: Option<u32>,
    build_error: Option<String>,
}

impl BuildProgress {
    /// Create an empty parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an original feature id so its build step can be labelled with the
    /// friendly id rather than the sanitized marker. Idempotent.
    pub fn register_feature(&mut self, id: &str) {
        self.feature_names
            .insert(DockerfileGenerator::sanitize_feature_id(id), id.to_string());
    }

    /// Register many feature ids at once (see [`Self::register_feature`]).
    pub fn register_features<I, S>(&mut self, ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for id in ids {
            self.register_feature(id.as_ref());
        }
    }

    /// Feed one line of BuildKit output. Trailing newlines/carriage returns are
    /// tolerated; unrecognized lines are ignored.
    pub fn push_line(&mut self, raw: &str) {
        let line = raw.trim_end_matches(['\r', '\n']);

        // Top-level (non-`#N`) failure summary emitted after the error block.
        if line.starts_with("ERROR: failed to build") {
            if self.build_error.is_none() {
                self.build_error = Some(line.to_string());
            }
            return;
        }

        // Every step line is `#<N> <tail>`.
        let Some(rest) = line.strip_prefix('#') else {
            return;
        };
        let Some((num, tail)) = rest.split_once(' ') else {
            return;
        };
        let Ok(id) = num.parse::<u32>() else {
            return;
        };
        // `#0 building with "…" instance …` is the driver preamble, not a step.
        if id == 0 {
            return;
        }

        // Terminal / status markers apply to an already-headered step.
        if tail == "CACHED" {
            let step = self.step_mut(id);
            step.status = StepStatus::Cached;
            return;
        }
        if tail == "DONE" || tail.starts_with("DONE ") {
            let elapsed = parse_done_secs(tail);
            let step = self.step_mut(id);
            step.elapsed_secs = elapsed;
            // Never demote a failed step.
            if step.status != StepStatus::Error {
                step.status = StepStatus::Done;
            }
            return;
        }
        if tail.starts_with("ERROR") {
            let step = self.step_mut(id);
            step.status = StepStatus::Error;
            step.log.push(tail.to_string());
            self.failing_id = Some(id);
            return;
        }

        // First sighting of `#N` is its header (bracketed or bare); any later
        // non-status line is captured output.
        if !self.index.contains_key(&id) {
            let (stage, op) = split_header(tail);
            let feature = self.recover_feature(&op);
            let label = feature.clone().unwrap_or_else(|| op.clone());
            self.push_step(BuildStep {
                id,
                label,
                op,
                stage,
                status: StepStatus::Running,
                elapsed_secs: None,
                feature,
                log: Vec::new(),
            });
            return;
        }

        let content = strip_elapsed_prefix(tail).to_string();
        self.step_mut(id).log.push(content);
    }

    /// All steps in encounter order.
    pub fn steps(&self) -> &[BuildStep] {
        &self.steps
    }

    /// Only the feature-install steps, in encounter order.
    pub fn feature_steps(&self) -> impl Iterator<Item = &BuildStep> {
        self.steps.iter().filter(|s| s.is_feature())
    }

    /// The step that failed, if any.
    pub fn failing_step(&self) -> Option<&BuildStep> {
        self.failing_id
            .and_then(|id| self.index.get(&id))
            .map(|&i| &self.steps[i])
    }

    /// The failing step's captured output, tail-capped to
    /// [`FAILING_LOG_TAIL_LINES`] lines, for a compact failure message.
    pub fn failing_step_log(&self) -> Option<String> {
        let step = self.failing_step()?;
        let start = step.log.len().saturating_sub(FAILING_LOG_TAIL_LINES);
        Some(step.log[start..].join("\n"))
    }

    /// The top-level `ERROR: failed to build …` summary, if the build failed.
    pub fn build_error(&self) -> Option<&str> {
        self.build_error.as_deref()
    }

    fn push_step(&mut self, step: BuildStep) {
        self.index.insert(step.id, self.steps.len());
        self.steps.push(step);
    }

    /// Get the step for `id`, creating a bare placeholder if a status marker
    /// arrives before its header (defensive — BuildKit emits headers first).
    fn step_mut(&mut self, id: u32) -> &mut BuildStep {
        if let Some(&i) = self.index.get(&id) {
            return &mut self.steps[i];
        }
        self.push_step(BuildStep {
            id,
            label: String::new(),
            op: String::new(),
            stage: None,
            status: StepStatus::Running,
            elapsed_secs: None,
            feature: None,
            log: Vec::new(),
        });
        let i = self.steps.len() - 1;
        &mut self.steps[i]
    }

    /// Recover the friendly feature id from a `RUN --mount=…,source=<dir>,…` op.
    fn recover_feature(&self, op: &str) -> Option<String> {
        let dir = extract_source_dir(op)?;
        let sanitized = strip_level_suffix(dir);
        Some(
            self.feature_names
                .get(sanitized)
                .cloned()
                .unwrap_or_else(|| sanitized.to_string()),
        )
    }
}

/// Split a step header `tail` into `(stage, op)`. `[stage-0 2/3] RUN …` →
/// `(Some("stage-0"), "RUN …")`; a bare `exporting to image` → `(None, "exporting to image")`.
fn split_header(tail: &str) -> (Option<String>, String) {
    if let Some(rest) = tail.strip_prefix('[') {
        if let Some((bracket, op)) = rest.split_once(']') {
            let stage = bracket.split_whitespace().next().map(str::to_string);
            return (stage, op.trim_start().to_string());
        }
    }
    (None, tail.to_string())
}

/// Extract `<dir>` from the first `,source=<dir>,` mount clause in `op`.
fn extract_source_dir(op: &str) -> Option<&str> {
    let after = &op[op.find(SOURCE_MARKER)? + SOURCE_MARKER.len()..];
    let end = after.find(',').unwrap_or(after.len());
    let dir = &after[..end];
    (!dir.is_empty()).then_some(dir)
}

/// Strip a trailing `_<level>` (all-digit) suffix: `node_0` → `node`,
/// `ai-clis_0` → `ai-clis`. Leaves a dir with no numeric suffix untouched.
fn strip_level_suffix(dir: &str) -> &str {
    if let Some(pos) = dir.rfind('_') {
        let suffix = &dir[pos + 1..];
        if !suffix.is_empty() && pos > 0 && suffix.bytes().all(|b| b.is_ascii_digit()) {
            return &dir[..pos];
        }
    }
    dir
}

/// Parse the seconds out of a `DONE <t>s` marker.
fn parse_done_secs(tail: &str) -> Option<f64> {
    tail.strip_prefix("DONE")?
        .trim()
        .strip_suffix('s')?
        .trim()
        .parse::<f64>()
        .ok()
}

/// Drop a leading elapsed token (`0.133 …` → `…`); lines without one are kept whole.
fn strip_elapsed_prefix(tail: &str) -> &str {
    if let Some((first, rest)) = tail.split_once(' ') {
        if first.parse::<f64>().is_ok() {
            return rest;
        }
    }
    tail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUCCESS: &str =
        include_str!("../../tests/fixtures/buildkit/feature_build_success.plain.log");
    const FAILURE: &str =
        include_str!("../../tests/fixtures/buildkit/feature_build_failure.plain.log");

    fn parse(log: &str) -> BuildProgress {
        let mut bp = BuildProgress::new();
        bp.register_features(["node", "ai-clis"]);
        for line in log.lines() {
            bp.push_line(line);
        }
        bp
    }

    #[test]
    fn success_extracts_both_feature_steps_as_done() {
        let bp = parse(SUCCESS);
        let features: Vec<_> = bp.feature_steps().collect();
        assert_eq!(features.len(), 2, "expected node + ai-clis feature steps");

        let node = &features[0];
        assert_eq!(node.id, 8);
        assert_eq!(node.feature.as_deref(), Some("node"));
        assert_eq!(node.label, "node");
        assert_eq!(node.status, StepStatus::Done);
        assert_eq!(node.stage.as_deref(), Some("stage-0"));

        let ai = &features[1];
        assert_eq!(ai.id, 9);
        assert_eq!(ai.feature.as_deref(), Some("ai-clis"));
        assert_eq!(ai.status, StepStatus::Done);
    }

    #[test]
    fn success_has_no_failing_step() {
        let bp = parse(SUCCESS);
        assert!(bp.failing_step().is_none());
        assert!(bp.failing_step_log().is_none());
        assert!(bp.build_error().is_none());
    }

    #[test]
    fn from_step_is_cached_and_export_is_done() {
        let bp = parse(SUCCESS);
        let from = bp.steps().iter().find(|s| s.id == 5).unwrap();
        assert_eq!(from.status, StepStatus::Cached);
        assert!(from.op.starts_with("FROM"), "op was {:?}", from.op);
        assert!(!from.is_feature());

        // The bare (unbracketed) export step is tracked and completes.
        let export = bp.steps().iter().find(|s| s.id == 10).unwrap();
        assert_eq!(export.status, StepStatus::Done);
        assert_eq!(export.label, "exporting to image");
    }

    #[test]
    fn done_elapsed_is_parsed() {
        let bp = parse(SUCCESS);
        let node = bp.steps().iter().find(|s| s.id == 8).unwrap();
        assert_eq!(node.elapsed_secs, Some(0.2));
    }

    #[test]
    fn preamble_step_zero_is_skipped() {
        let bp = parse(SUCCESS);
        assert!(bp.steps().iter().all(|s| s.id != 0));
    }

    #[test]
    fn failure_isolates_failing_step() {
        let bp = parse(FAILURE);

        // node succeeded before the failing feature.
        let node = bp.steps().iter().find(|s| s.id == 8).unwrap();
        assert_eq!(node.status, StepStatus::Done);

        let failing = bp.failing_step().expect("a failing step");
        assert_eq!(failing.id, 9);
        assert_eq!(failing.feature.as_deref(), Some("ai-clis"));
        assert_eq!(failing.status, StepStatus::Error);

        let log = bp.failing_step_log().expect("failing step log");
        // Contains this feature's own output + the error, not node's.
        assert!(log.contains("npm: not found"), "log was:\n{log}");
        assert!(log.contains("exit code: 127"), "log was:\n{log}");
        assert!(
            !log.contains("Installing node feature"),
            "failing log must not bleed the node step:\n{log}"
        );
        // Elapsed prefix is stripped from captured output.
        assert!(
            log.contains("Installing ai-clis feature..."),
            "log was:\n{log}"
        );

        assert!(
            bp.build_error()
                .is_some_and(|e| e.contains("failed to build"))
        );
    }

    #[test]
    fn feature_mapping_reverses_oci_sanitized_marker() {
        // OCI ids sanitize lossily (`.`/`/`/`:` -> `_`); registration lets the
        // parser recover the original id from the build-log marker.
        let id = "ghcr.io/devcontainers/features/node:1";
        let sanitized = DockerfileGenerator::sanitize_feature_id(id);
        let mut bp = BuildProgress::new();
        bp.register_feature(id);
        bp.push_line(&format!(
            "#8 [stage-0 2/3] RUN --mount=type=bind,from=dev_containers_feature_content_source,source={sanitized}_0,target=/tmp/build-features-0/{sanitized}_0,rw     ./install.sh"
        ));
        bp.push_line("#8 DONE 1.0s");

        let step = bp.feature_steps().next().expect("a feature step");
        assert_eq!(step.feature.as_deref(), Some(id));
        assert_eq!(step.label, id);
    }

    #[test]
    fn unregistered_feature_falls_back_to_sanitized_id() {
        let mut bp = BuildProgress::new();
        // No register_feature call.
        bp.push_line("#8 [stage-0 2/3] RUN --mount=type=bind,from=dev_containers_feature_content_source,source=common-utils_1,target=/t,rw ./install.sh");
        let step = bp.feature_steps().next().expect("a feature step");
        assert_eq!(step.feature.as_deref(), Some("common-utils"));
    }

    #[test]
    fn failing_step_log_is_tail_capped() {
        let mut bp = BuildProgress::new();
        bp.push_line("#8 [stage-0 2/3] RUN --mount=type=bind,from=dev_containers_feature_content_source,source=noisy_0,target=/t,rw ./install.sh");
        for i in 0..200 {
            bp.push_line(&format!("#8 0.{i:03} line number {i}"));
        }
        bp.push_line("#8 ERROR: process \"…\" did not complete successfully: exit code: 1");

        let log = bp.failing_step_log().unwrap();
        let lines = log.lines().count();
        assert_eq!(lines, FAILING_LOG_TAIL_LINES);
        // The error line (last pushed) survives the tail cap.
        assert!(log.contains("exit code: 1"));
    }

    #[test]
    fn non_step_lines_are_ignored() {
        let mut bp = BuildProgress::new();
        bp.push_line("");
        bp.push_line("------");
        bp.push_line("Dockerfile.fail:8");
        bp.push_line("   8 | >>> RUN --mount=…");
        bp.push_line(" > [stage-0 3/3] RUN …:");
        assert!(bp.steps().is_empty());
    }
}
