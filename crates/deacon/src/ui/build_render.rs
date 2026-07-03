//! Build-output rendering: resolve a [`BuildOutputMode`] and drive a live
//! renderer for `docker build` output.
//!
//! The core streaming executor ([`deacon_core::docker_retry::run_build_with_retry`])
//! forwards each output line to a [`BuildLineSink`]. This module provides the
//! deacon-side sinks that turn those lines into UI:
//!
//! * **Compact** (default, interactive TTY) — parse BuildKit `--progress=plain`
//!   with [`BuildProgress`] and render a collapsing per-step line via
//!   `indicatif::MultiProgress`. On failure, print only the failing step's log
//!   tail instead of the whole firehose.
//! * **Plain** (non-TTY / CI / `--log-format json` / `--progress json`) — stream
//!   each line straight to stderr, verbatim, as it arrives.
//! * **Inherit** (`-v`/verbose on a TTY) — no renderer at all; the caller hands
//!   the terminal to buildx (see [`BuildIo::Inherited`]) for its native UI.
//!
//! The renderer types use interior mutability so they satisfy [`BuildLineSink`]'s
//! `&self` contract (the executor shares the sink across retry attempts).

use std::collections::HashSet;
use std::sync::Mutex;

use console::style;
use deacon_core::build::BuildOutputMode;
use deacon_core::docker_retry::{BuildIo, BuildLineSink, BuildStream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use super::build_progress::{BuildProgress, StepStatus};

/// Resolve the build-output mode from the same signals the CLI already computes
/// for logging and spinners.
///
/// * `verbose` — repeated `-v` count (0 = quiet default).
/// * `stderr_is_tty` — whether stderr is an interactive terminal.
/// * `json_format` — whether the effective log format is JSON.
/// * `progress_is_auto` — whether `--progress` is left at `auto` (an explicit
///   `--progress json`/`none` forces Plain so the structured/`quiet` contract is
///   honored).
///
/// Matrix (mirrors the build-output plan):
/// * non-TTY, or JSON, or non-auto progress → **Plain**
/// * TTY + `-v`/verbose → **Inherit**
/// * TTY, default verbosity → **Compact**
pub fn resolve_build_output_mode(
    verbose: u8,
    stderr_is_tty: bool,
    json_format: bool,
    progress_is_auto: bool,
) -> BuildOutputMode {
    if !stderr_is_tty || json_format || !progress_is_auto {
        return BuildOutputMode::Plain;
    }
    if verbose > 0 {
        return BuildOutputMode::Inherit;
    }
    BuildOutputMode::Compact
}

/// A build-output renderer that implements [`BuildLineSink`]. Constructed via
/// [`BuildRenderer::for_mode`]; `None` for [`BuildOutputMode::Inherit`] (which
/// has no sink — the terminal is handed to buildx).
pub enum BuildRenderer {
    /// Stream lines verbatim to stderr.
    Plain(PlainRenderer),
    /// Collapsing per-step `MultiProgress` UI with failure-trim. Boxed because
    /// it is much larger than the `Plain` variant (clippy `large_enum_variant`).
    Compact(Box<CompactRenderer>),
}

impl BuildRenderer {
    /// Build the renderer for `mode`, registering `feature_ids` so feature-install
    /// steps can be labelled with their friendly ids. Returns `None` for
    /// [`BuildOutputMode::Inherit`].
    pub fn for_mode<I, S>(mode: BuildOutputMode, feature_ids: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        match mode {
            BuildOutputMode::Inherit => None,
            BuildOutputMode::Plain => Some(Self::Plain(PlainRenderer::new())),
            BuildOutputMode::Compact => {
                Some(Self::Compact(Box::new(CompactRenderer::new(feature_ids))))
            }
        }
    }

    /// Finalize the render after the build completes. Compact clears its live
    /// spinner and, on failure, prints the failing step's log tail. No-op for
    /// Plain (lines already streamed).
    pub fn finish(&self, success: bool) {
        if let Self::Compact(c) = self {
            c.finish(success);
        }
    }
}

impl BuildLineSink for BuildRenderer {
    fn on_line(&self, line: &str, stream: BuildStream) {
        match self {
            Self::Plain(p) => p.on_line(line, stream),
            Self::Compact(c) => c.on_line(line, stream),
        }
    }
    fn reset(&self) {
        match self {
            Self::Plain(p) => p.reset(),
            Self::Compact(c) => c.reset(),
        }
    }
}

/// The [`BuildIo`] to pass to the executor for an optional renderer: `Inherited`
/// when there is no renderer (Inherit mode), otherwise `Captured(Some(..))`.
pub fn io_for(renderer: &Option<BuildRenderer>) -> BuildIo<'_> {
    match renderer {
        None => BuildIo::Inherited,
        Some(r) => BuildIo::Captured(Some(r)),
    }
}

/// Plain renderer: echo each output line to stderr verbatim as it streams.
pub struct PlainRenderer;

impl PlainRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlainRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildLineSink for PlainRenderer {
    fn on_line(&self, line: &str, _stream: BuildStream) {
        // Verbatim to stderr — stdout stays reserved for the command's result
        // (JSON purity contract). `eprintln!` already targets stderr.
        eprintln!("{}", line);
    }
    fn reset(&self) {
        // Verbatim streaming has no state to discard across retries.
    }
}

/// Interior-mutable state for the compact renderer, guarded by a single mutex so
/// the whole per-line update is atomic.
struct CompactState {
    progress: BuildProgress,
    /// Step ids already announced as finished (avoids re-printing on each line).
    announced: HashSet<u32>,
    /// The single live spinner for the currently-running step.
    active: Option<ProgressBar>,
    /// Id of the step the active spinner currently represents.
    active_id: Option<u32>,
}

/// Compact renderer: drive [`BuildProgress`] from the line stream and render a
/// collapsing per-step view. Completed steps print a static one-line summary
/// above a single live spinner tracking the current step.
pub struct CompactRenderer {
    multi: MultiProgress,
    /// Friendly feature ids, retained so [`Self::reset`] can re-register them on
    /// a fresh parser after a transient-retry.
    feature_ids: Vec<String>,
    state: Mutex<CompactState>,
}

impl CompactRenderer {
    /// Create a renderer, registering `feature_ids` so feature-install steps are
    /// labelled with their friendly ids rather than the sanitized mount marker.
    pub fn new<I, S>(feature_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let feature_ids: Vec<String> = feature_ids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        let mut progress = BuildProgress::new();
        progress.register_features(&feature_ids);
        Self {
            multi: MultiProgress::new(),
            feature_ids,
            state: Mutex::new(CompactState {
                progress,
                announced: HashSet::new(),
                active: None,
                active_id: None,
            }),
        }
    }

    fn spinner_style() -> ProgressStyle {
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
    }

    /// Finalize: clear the live spinner and, on failure, print the failing step's
    /// captured log tail (or the top-level build error).
    pub fn finish(&self, success: bool) {
        let mut st = self.state.lock().unwrap();
        if let Some(bar) = st.active.take() {
            bar.finish_and_clear();
        }
        st.active_id = None;

        if !success {
            let trim = st.progress.failing_step_log();
            let build_error = st.progress.build_error().map(str::to_string);
            let failing_label = st
                .progress
                .failing_step()
                .map(|s| s.label.clone())
                .unwrap_or_else(|| "build".to_string());
            drop(st);

            let header = style(format!("✗ {} failed", failing_label)).red().bold();
            let _ = self.multi.println(header.to_string());
            if let Some(trim) = trim {
                for line in trim.lines() {
                    let _ = self.multi.println(format!("    {}", line));
                }
            } else if let Some(err) = build_error {
                let _ = self.multi.println(format!("    {}", err));
            }
        }
    }
}

impl BuildLineSink for CompactRenderer {
    fn on_line(&self, line: &str, _stream: BuildStream) {
        let mut st = self.state.lock().unwrap();
        st.progress.push_line(line);

        // Snapshot the fields we need so we can release the parser borrow before
        // touching the (separately-borrowed) render fields.
        struct StepView {
            id: u32,
            label: String,
            status: StepStatus,
            elapsed: Option<f64>,
        }
        let steps: Vec<StepView> = st
            .progress
            .steps()
            .iter()
            .map(|s| StepView {
                id: s.id,
                label: s.label.clone(),
                status: s.status,
                elapsed: s.elapsed_secs,
            })
            .collect();

        // Announce any newly-finished steps as static lines above the spinner.
        for step in &steps {
            let terminal = matches!(
                step.status,
                StepStatus::Done | StepStatus::Cached | StepStatus::Error
            );
            if terminal && !st.announced.contains(&step.id) {
                st.announced.insert(step.id);
                let _ = self.multi.println(format_finished(
                    step.label.as_str(),
                    step.status,
                    step.elapsed,
                ));
            }
        }

        // Track the newest still-running step on the single live spinner.
        let running = steps
            .iter()
            .rev()
            .find(|s| matches!(s.status, StepStatus::Running));
        match running {
            Some(step) => {
                if st.active.is_none() {
                    let bar = self.multi.add(ProgressBar::new_spinner());
                    bar.set_style(Self::spinner_style());
                    st.active = Some(bar);
                }
                if st.active_id != Some(step.id) {
                    st.active_id = Some(step.id);
                    if let Some(bar) = &st.active {
                        bar.set_message(style(step.label.clone()).cyan().to_string());
                    }
                }
                if let Some(bar) = &st.active {
                    bar.tick();
                }
            }
            None => {
                if let Some(bar) = st.active.take() {
                    bar.finish_and_clear();
                }
                st.active_id = None;
            }
        }
    }

    fn reset(&self) {
        // A transient failure was retried: discard parsed state and printed steps
        // so the new attempt renders from scratch.
        let mut st = self.state.lock().unwrap();
        if let Some(bar) = st.active.take() {
            bar.finish_and_clear();
        }
        st.active_id = None;
        st.announced.clear();
        let mut fresh = BuildProgress::new();
        fresh.register_features(&self.feature_ids);
        st.progress = fresh;
    }
}

/// Format a finished step as a single static line with a status glyph.
fn format_finished(label: &str, status: StepStatus, elapsed: Option<f64>) -> String {
    let suffix = match elapsed {
        Some(secs) => format!(" ({:.1}s)", secs),
        None => String::new(),
    };
    match status {
        StepStatus::Done => format!("{} {}{}", style("✓").green(), label, style(suffix).dim()),
        StepStatus::Cached => format!(
            "{} {}{}",
            style("⊘").blue(),
            label,
            style(" (cached)").dim()
        ),
        StepStatus::Error => format!("{} {}{}", style("✗").red(), style(label).red(), suffix),
        StepStatus::Running => format!("… {}{}", label, suffix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_plain_when_not_tty() {
        // Non-TTY always plain, regardless of verbosity.
        assert_eq!(
            resolve_build_output_mode(0, false, false, true),
            BuildOutputMode::Plain
        );
        assert_eq!(
            resolve_build_output_mode(2, false, false, true),
            BuildOutputMode::Plain
        );
    }

    #[test]
    fn resolve_plain_when_json() {
        assert_eq!(
            resolve_build_output_mode(0, true, true, true),
            BuildOutputMode::Plain
        );
    }

    #[test]
    fn resolve_plain_when_progress_not_auto() {
        // Explicit --progress json/none forces plain even on a TTY.
        assert_eq!(
            resolve_build_output_mode(0, true, false, false),
            BuildOutputMode::Plain
        );
    }

    #[test]
    fn resolve_inherit_when_verbose_tty() {
        assert_eq!(
            resolve_build_output_mode(1, true, false, true),
            BuildOutputMode::Inherit
        );
        assert_eq!(
            resolve_build_output_mode(3, true, false, true),
            BuildOutputMode::Inherit
        );
    }

    #[test]
    fn resolve_compact_default_tty() {
        assert_eq!(
            resolve_build_output_mode(0, true, false, true),
            BuildOutputMode::Compact
        );
    }

    #[test]
    fn for_mode_inherit_has_no_renderer() {
        let r = BuildRenderer::for_mode(BuildOutputMode::Inherit, Vec::<String>::new());
        assert!(r.is_none());
        assert!(matches!(io_for(&r), BuildIo::Inherited));
    }

    #[test]
    fn for_mode_plain_and_compact_have_renderers() {
        let p = BuildRenderer::for_mode(BuildOutputMode::Plain, Vec::<String>::new());
        assert!(matches!(p, Some(BuildRenderer::Plain(_))));
        assert!(matches!(io_for(&p), BuildIo::Captured(Some(_))));

        let c = BuildRenderer::for_mode(BuildOutputMode::Compact, ["node", "ai-clis"]);
        assert!(matches!(c, Some(BuildRenderer::Compact(_))));
        assert!(matches!(io_for(&c), BuildIo::Captured(Some(_))));
    }

    #[test]
    fn format_finished_glyphs() {
        // Sanity: labels are present and distinct per status (color codes vary by
        // terminal detection, so assert on the substring, not exact bytes).
        assert!(format_finished("node", StepStatus::Done, Some(1.25)).contains("node"));
        assert!(format_finished("node", StepStatus::Cached, None).contains("cached"));
        assert!(format_finished("ai-clis", StepStatus::Error, None).contains("ai-clis"));
    }

    #[test]
    fn compact_renderer_consumes_failure_fixture_without_panicking() {
        // Drive the compact renderer over the B1 failure fixture end-to-end and
        // finalize — exercises push/announce/spinner/finish + failure-trim paths.
        // MultiProgress writes to a hidden target under `cargo test` (not a TTY),
        // so this asserts the state machine doesn't panic and the parser saw the
        // failure.
        let log = include_str!("../../tests/fixtures/buildkit/feature_build_failure.plain.log");
        let renderer = CompactRenderer::new(["node", "ai-clis"]);
        for line in log.lines() {
            renderer.on_line(line, BuildStream::Stderr);
        }
        {
            let st = renderer.state.lock().unwrap();
            assert!(
                st.progress.failing_step_log().is_some(),
                "failure fixture should yield a failing-step log"
            );
        }
        renderer.finish(false);
    }
}
