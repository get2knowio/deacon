//! Shared progress callback helpers for command implementations.

use anyhow::Result;
use deacon_core::progress::{ProgressEvent, ProgressTracker};
use std::sync::{Arc, Mutex};
use tracing::warn;

/// RAII guard that suspends the interactive progress spinner for its lifetime,
/// resuming it on drop.
///
/// The streaming build-output renderer (`ui::build_render`) draws its own
/// progress to stderr; if the general `up` spinner keeps its steady-tick going
/// at the same time, the two clobber each other. Wrap a build call in one of
/// these so the spinner yields stderr for the duration of the build:
///
/// ```ignore
/// let _pause = SpinnerPause::new(&args.progress_tracker);
/// let result = build_image_with_features(...).await;
/// drop(_pause); // (or let it drop at end of scope)
/// ```
///
/// A non-interactive tracker (JSON/none/no emitter) suspends to a no-op, so this
/// is always safe to acquire.
pub struct SpinnerPause<'a> {
    tracker: &'a Arc<Mutex<Option<ProgressTracker>>>,
}

impl<'a> SpinnerPause<'a> {
    /// Suspend the spinner now; it resumes when the returned guard drops.
    pub fn new(tracker: &'a Arc<Mutex<Option<ProgressTracker>>>) -> Self {
        match tracker.lock() {
            Ok(mut guard) => {
                if let Some(t) = guard.as_mut() {
                    t.suspend();
                }
            }
            Err(e) => warn!("Progress tracker mutex poisoned (suspend): {}", e),
        }
        Self { tracker }
    }
}

impl Drop for SpinnerPause<'_> {
    fn drop(&mut self) {
        match self.tracker.lock() {
            Ok(mut guard) => {
                if let Some(t) = guard.as_mut() {
                    t.resume();
                }
            }
            Err(e) => warn!("Progress tracker mutex poisoned (resume): {}", e),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::progress::{ProgressEmitter, ProgressEvent};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A ProgressEmitter that counts suspend/resume calls so we can assert the
    /// `SpinnerPause` guard actually drives them through the tracker.
    #[derive(Debug, Default)]
    struct CountingEmitter {
        suspends: Arc<AtomicUsize>,
        resumes: Arc<AtomicUsize>,
    }

    impl ProgressEmitter for CountingEmitter {
        fn emit(&mut self, _event: &ProgressEvent) -> deacon_core::progress::Result<()> {
            Ok(())
        }
        fn suspend(&mut self) {
            self.suspends.fetch_add(1, Ordering::SeqCst);
        }
        fn resume(&mut self) {
            self.resumes.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn spinner_pause_suspends_on_new_and_resumes_on_drop() {
        let suspends = Arc::new(AtomicUsize::new(0));
        let resumes = Arc::new(AtomicUsize::new(0));
        let emitter = CountingEmitter {
            suspends: suspends.clone(),
            resumes: resumes.clone(),
        };
        let tracker = ProgressTracker::new(Some(Box::new(emitter)), None, Default::default())
            .expect("tracker");
        let tracker = Arc::new(Mutex::new(Some(tracker)));

        {
            let _pause = SpinnerPause::new(&tracker);
            assert_eq!(suspends.load(Ordering::SeqCst), 1, "suspend on new");
            assert_eq!(resumes.load(Ordering::SeqCst), 0, "no resume yet");
        }
        assert_eq!(resumes.load(Ordering::SeqCst), 1, "resume on drop");
    }

    #[test]
    fn spinner_pause_is_safe_with_no_tracker() {
        // A `None` tracker (non-interactive) must not panic.
        let tracker = Arc::new(Mutex::new(None));
        {
            let _pause = SpinnerPause::new(&tracker);
        }
    }
}
