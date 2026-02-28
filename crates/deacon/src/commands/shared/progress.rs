//! Shared progress callback helpers for command implementations.

use anyhow::Result;
use deacon_core::progress::{ProgressEvent, ProgressTracker};
use std::sync::{Arc, Mutex};
use tracing::warn;

/// Create a progress event callback that emits events through the shared progress tracker.
///
/// Returns a closure suitable for passing to lifecycle execution functions as a progress callback.
/// Standardizes on the explicit `match` + `warn!` pattern for mutex poisoning (no silent fallback).
pub fn make_progress_callback(
    tracker: &Arc<Mutex<Option<ProgressTracker>>>,
) -> impl Fn(ProgressEvent) -> Result<()> + '_ {
    move |event: ProgressEvent| -> Result<()> {
        match tracker.lock() {
            Ok(mut tracker_guard) => {
                if let Some(ref mut tracker) = tracker_guard.as_mut() {
                    tracker.emit_event(event)?;
                }
            }
            Err(e) => {
                warn!("Progress tracker mutex poisoned: {}", e);
            }
        }
        Ok(())
    }
}
