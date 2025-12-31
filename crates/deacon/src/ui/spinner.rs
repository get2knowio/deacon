use anyhow::Result;
use console::style;
use deacon_core::progress::{ProgressEmitter, ProgressEvent};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

// (no-op) TTY helpers live in CLI; UI module remains pure

fn default_style() -> ProgressStyle {
    // Use a green spinner and leave message coloring to message composition
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
}

fn success_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg}").unwrap()
}

/// A spinner that maps ProgressEvent stream to friendly messages on stderr.
#[derive(Debug)]
pub struct SpinnerEmitter {
    pb: ProgressBar,
    last_phase: Option<String>,
}

impl SpinnerEmitter {
    pub fn new() -> Self {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_style(default_style());
        Self {
            pb,
            last_phase: None,
        }
    }

    fn set_msg(&self, msg: impl Into<String>) {
        self.pb.set_message(msg.into());
    }

    fn finish_with(&self, msg: impl Into<String>) {
        let msg = msg.into();
        self.pb.set_style(success_style());
        self.pb.finish_with_message(msg);
    }
}

impl Drop for SpinnerEmitter {
    fn drop(&mut self) {
        if !self.pb.is_finished() {
            self.pb.finish_and_clear();
        }
    }
}

impl ProgressEmitter for SpinnerEmitter {
    fn emit(&mut self, event: &ProgressEvent) -> Result<()> {
        use ProgressEvent::*;
        match event {
            ContainerCreateBegin { name, .. } => {
                let suffix = if name.is_empty() {
                    String::new()
                } else {
                    format!(" '{}'", name)
                };
                self.set_msg(format!(
                    "{}",
                    style(format!("Creating container{}…", suffix)).yellow()
                ));
            }
            ContainerCreateEnd {
                success,
                duration_ms,
                ..
            } => {
                if *success {
                    self.finish_with(format!(
                        "{}",
                        style(format!("Container ready in {} ms", duration_ms)).green()
                    ));
                } else {
                    self.finish_with(format!("{}", style("Container creation failed").red()));
                }
            }
            LifecyclePhaseBegin { phase, .. } => {
                self.last_phase = Some(phase.clone());
                self.set_msg(format!("{}", style(format!("Running {}…", phase)).yellow()));
            }
            LifecyclePhaseEnd {
                phase,
                success,
                duration_ms,
                ..
            } => {
                let status = if *success { "completed" } else { "failed" };
                let msg = format!("{} {} in {} ms", phase, status, duration_ms);
                if *success {
                    self.set_msg(style(msg).green().to_string());
                } else {
                    self.set_msg(style(msg).red().to_string());
                }
            }
            LifecycleCommandBegin {
                phase, command_id, ..
            } => {
                // For brevity, only show command ID under current phase on TTY
                let _ = (&self.last_phase, phase, command_id);
            }
            LifecycleCommandEnd { .. } => {
                // Keep spinner minimal; phase end will summarize
            }
            FeaturesInstallBegin { feature_id, .. } => {
                self.set_msg(
                    style(format!("Installing feature {}…", feature_id))
                        .yellow()
                        .to_string(),
                );
            }
            FeaturesInstallEnd {
                feature_id,
                success,
                duration_ms,
                ..
            } => {
                let status = if *success { "installed" } else { "failed" };
                let msg = format!("Feature {} {} in {} ms", feature_id, status, duration_ms);
                if *success {
                    self.set_msg(style(msg).green().to_string());
                } else {
                    self.set_msg(style(msg).red().to_string());
                }
            }
            BuildBegin { context, .. } => {
                self.set_msg(
                    style(format!("Building image (context: {})…", context))
                        .yellow()
                        .to_string(),
                );
            }
            BuildEnd {
                success,
                duration_ms,
                ..
            } => {
                let status = if *success { "built" } else { "failed" };
                let msg = format!("Image {} in {} ms", status, duration_ms);
                if *success {
                    self.set_msg(style(msg).green().to_string());
                } else {
                    self.set_msg(style(msg).red().to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Simple RAII spinner for wrapping a synchronous/async operation without events
pub struct PlainSpinner {
    pb: ProgressBar,
    finished: bool,
}

impl PlainSpinner {
    pub fn start(message: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_style(default_style());
        pb.set_message(style(message).yellow().to_string());
        Self {
            pb,
            finished: false,
        }
    }

    pub fn finish_with_message(mut self, message: &str) {
        self.pb.set_style(success_style());
        self.pb
            .finish_with_message(style(message).green().to_string());
        self.finished = true;
    }

    pub fn fail_with_message(mut self, message: &str) {
        self.pb.set_style(success_style());
        self.pb
            .finish_with_message(style(message).red().to_string());
        self.finished = true;
    }
}

impl Drop for PlainSpinner {
    fn drop(&mut self) {
        if !self.finished {
            self.pb.finish_and_clear();
        }
    }
}

impl Default for SpinnerEmitter {
    fn default() -> Self {
        Self::new()
    }
}
