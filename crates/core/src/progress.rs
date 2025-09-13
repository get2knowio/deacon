//! Progress event streaming, metrics collection, and audit logging
//!
//! This module provides structured progress events for major operations,
//! in-memory metrics collection with histogram aggregation, and persistent
//! audit logging with rotation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, instrument, warn};

/// Global event ID counter for deterministic ordering
static EVENT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Progress event types for different phases
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ProgressEvent {
    /// Build phase events
    #[serde(rename = "build.begin")]
    BuildBegin {
        id: u64,
        timestamp: u64,
        context: String,
        dockerfile: Option<String>,
    },
    #[serde(rename = "build.end")]
    BuildEnd {
        id: u64,
        timestamp: u64,
        context: String,
        duration_ms: u64,
        success: bool,
        image_id: Option<String>,
    },

    /// Container lifecycle events
    #[serde(rename = "container.create.begin")]
    ContainerCreateBegin {
        id: u64,
        timestamp: u64,
        name: String,
        image: String,
    },
    #[serde(rename = "container.create.end")]
    ContainerCreateEnd {
        id: u64,
        timestamp: u64,
        name: String,
        duration_ms: u64,
        success: bool,
        container_id: Option<String>,
    },

    /// Feature installation events
    #[serde(rename = "features.install.begin")]
    FeaturesInstallBegin {
        id: u64,
        timestamp: u64,
        feature_id: String,
        version: Option<String>,
    },
    #[serde(rename = "features.install.end")]
    FeaturesInstallEnd {
        id: u64,
        timestamp: u64,
        feature_id: String,
        duration_ms: u64,
        success: bool,
    },

    /// Lifecycle phase events
    #[serde(rename = "lifecycle.phase.begin")]
    LifecyclePhaseBegin {
        id: u64,
        timestamp: u64,
        phase: String,
        commands: Vec<String>,
    },
    #[serde(rename = "lifecycle.phase.end")]
    LifecyclePhaseEnd {
        id: u64,
        timestamp: u64,
        phase: String,
        duration_ms: u64,
        success: bool,
    },
}

impl ProgressEvent {
    /// Get the event ID
    pub fn id(&self) -> u64 {
        match self {
            ProgressEvent::BuildBegin { id, .. } => *id,
            ProgressEvent::BuildEnd { id, .. } => *id,
            ProgressEvent::ContainerCreateBegin { id, .. } => *id,
            ProgressEvent::ContainerCreateEnd { id, .. } => *id,
            ProgressEvent::FeaturesInstallBegin { id, .. } => *id,
            ProgressEvent::FeaturesInstallEnd { id, .. } => *id,
            ProgressEvent::LifecyclePhaseBegin { id, .. } => *id,
            ProgressEvent::LifecyclePhaseEnd { id, .. } => *id,
        }
    }

    /// Get the event timestamp
    pub fn timestamp(&self) -> u64 {
        match self {
            ProgressEvent::BuildBegin { timestamp, .. } => *timestamp,
            ProgressEvent::BuildEnd { timestamp, .. } => *timestamp,
            ProgressEvent::ContainerCreateBegin { timestamp, .. } => *timestamp,
            ProgressEvent::ContainerCreateEnd { timestamp, .. } => *timestamp,
            ProgressEvent::FeaturesInstallBegin { timestamp, .. } => *timestamp,
            ProgressEvent::FeaturesInstallEnd { timestamp, .. } => *timestamp,
            ProgressEvent::LifecyclePhaseBegin { timestamp, .. } => *timestamp,
            ProgressEvent::LifecyclePhaseEnd { timestamp, .. } => *timestamp,
        }
    }
}

/// Duration histogram for collecting timing metrics
#[derive(Debug, Clone)]
pub struct DurationHistogram {
    buckets: Vec<Duration>,
    counts: Vec<u64>,
    total_count: u64,
    total_duration: Duration,
}

impl DurationHistogram {
    /// Create a new histogram with default buckets
    pub fn new() -> Self {
        let buckets = vec![
            Duration::from_millis(10),
            Duration::from_millis(50),
            Duration::from_millis(100),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(5),
            Duration::from_secs(10),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(300),
        ];
        let counts = vec![0; buckets.len() + 1]; // +1 for values > max bucket

        Self {
            buckets,
            counts,
            total_count: 0,
            total_duration: Duration::ZERO,
        }
    }

    /// Record a duration
    pub fn record(&mut self, duration: Duration) {
        self.total_count += 1;
        self.total_duration += duration;

        // Find the appropriate bucket
        let mut bucket_index = self.buckets.len(); // Default to overflow bucket
        for (i, &bucket_threshold) in self.buckets.iter().enumerate() {
            if duration <= bucket_threshold {
                bucket_index = i;
                break;
            }
        }
        self.counts[bucket_index] += 1;
    }

    /// Get summary statistics
    pub fn summary(&self) -> HistogramSummary {
        let avg_duration = if self.total_count > 0 {
            self.total_duration / self.total_count as u32
        } else {
            Duration::ZERO
        };

        HistogramSummary {
            count: self.total_count,
            total_duration: self.total_duration,
            avg_duration,
            buckets: self
                .buckets
                .iter()
                .zip(self.counts.iter())
                .map(|(threshold, count)| BucketSummary {
                    threshold_ms: threshold.as_millis() as u64,
                    count: *count,
                })
                .collect(),
            overflow_count: self.counts[self.buckets.len()],
        }
    }
}

impl Default for DurationHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for a histogram
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramSummary {
    pub count: u64,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub buckets: Vec<BucketSummary>,
    pub overflow_count: u64,
}

/// Summary for a single histogram bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketSummary {
    pub threshold_ms: u64,
    pub count: u64,
}

/// In-memory metrics collection
#[derive(Debug)]
pub struct Metrics {
    histograms: HashMap<String, DurationHistogram>,
}

impl Metrics {
    /// Create new metrics collection
    pub fn new() -> Self {
        Self {
            histograms: HashMap::new(),
        }
    }

    /// Record a duration for a specific operation
    pub fn record_duration(&mut self, operation: &str, duration: Duration) {
        let histogram = self.histograms.entry(operation.to_string()).or_default();
        histogram.record(duration);
        debug!("Recorded {} duration: {:?}", operation, duration);
    }

    /// Get summary of all metrics
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            histograms: self
                .histograms
                .iter()
                .map(|(name, histogram)| (name.clone(), histogram.summary()))
                .collect(),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of all collected metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub histograms: HashMap<String, HistogramSummary>,
}

/// Audit log writer with file rotation
#[derive(Debug)]
pub struct AuditLog {
    file: BufWriter<File>,
    current_size: u64,
    max_size: u64,
    log_path: PathBuf,
    rotation_count: u32,
}

impl AuditLog {
    /// Create a new audit log with rotation
    pub fn new(cache_dir: &Path, max_size: u64) -> Result<Self> {
        let log_path = cache_dir.join("audit.jsonl");

        // Ensure the cache directory exists
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let current_size = file.metadata()?.len();
        let file = BufWriter::new(file);

        Ok(Self {
            file,
            current_size,
            max_size,
            log_path,
            rotation_count: 0,
        })
    }

    /// Write an event to the audit log
    #[instrument(skip(self))]
    pub fn log_event(&mut self, event: &ProgressEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        writeln!(self.file, "{}", line)?;
        self.file.flush()?;

        self.current_size += line.len() as u64 + 1; // +1 for newline
        debug!(
            "Logged audit event {} to {}",
            event.id(),
            self.log_path.display()
        );

        // Check if rotation is needed
        if self.current_size >= self.max_size {
            self.rotate()?;
        }

        Ok(())
    }

    /// Rotate the audit log file
    fn rotate(&mut self) -> Result<()> {
        // Flush and close current file
        self.file.flush()?;

        // Move current file to rotated name
        let rotated_path = self
            .log_path
            .with_extension(format!("jsonl.{}", self.rotation_count));
        std::fs::rename(&self.log_path, &rotated_path)?;

        // Create new file
        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        self.file = BufWriter::new(new_file);
        self.current_size = 0;
        self.rotation_count += 1;

        debug!("Rotated audit log to {}", rotated_path.display());
        Ok(())
    }
}

/// Progress event emitter and metrics collector
#[derive(Debug)]
pub struct ProgressTracker {
    emitter: Option<Box<dyn ProgressEmitter>>,
    audit_log: Option<AuditLog>,
    metrics: Arc<Mutex<Metrics>>,
}

impl ProgressTracker {
    /// Create a new progress tracker with the specified emitter
    pub fn new(
        emitter: Option<Box<dyn ProgressEmitter>>,
        cache_dir: Option<&Path>,
    ) -> Result<Self> {
        let audit_log = if let Some(cache_dir) = cache_dir {
            Some(AuditLog::new(cache_dir, 10 * 1024 * 1024)?) // 10MB max size
        } else {
            None
        };

        Ok(Self {
            emitter,
            audit_log,
            metrics: Arc::new(Mutex::new(Metrics::new())),
        })
    }

    /// Emit a progress event
    pub fn emit_event(&mut self, event: ProgressEvent) -> Result<()> {
        // Emit to configured emitter
        if let Some(ref mut emitter) = self.emitter {
            emitter.emit(&event)?;
        }

        // Log to audit log
        if let Some(ref mut audit_log) = self.audit_log {
            audit_log.log_event(&event)?;
        }

        Ok(())
    }

    /// Record metrics for an operation
    pub fn record_duration(&self, operation: &str, duration: Duration) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.record_duration(operation, duration);
        }
    }

    /// Get metrics summary
    pub fn metrics_summary(&self) -> Option<MetricsSummary> {
        self.metrics.lock().ok().map(|metrics| metrics.summary())
    }

    /// Create a new event ID
    pub fn next_event_id() -> u64 {
        EVENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    /// Get current timestamp in milliseconds since Unix epoch
    pub fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Trait for progress event emission
pub trait ProgressEmitter: Send + Sync + std::fmt::Debug {
    /// Emit a progress event
    fn emit(&mut self, event: &ProgressEvent) -> Result<()>;
}

/// JSON line emitter that writes to a file
#[derive(Debug)]
pub struct JsonFileEmitter {
    writer: BufWriter<File>,
}

impl JsonFileEmitter {
    /// Create a new JSON file emitter
    pub fn new(file_path: &Path) -> Result<Self> {
        // Ensure the parent directory exists
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;

        Ok(Self {
            writer: BufWriter::new(file),
        })
    }
}

impl ProgressEmitter for JsonFileEmitter {
    fn emit(&mut self, event: &ProgressEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        writeln!(self.writer, "{}", line)?;
        self.writer.flush()?;
        Ok(())
    }
}

/// Standard output emitter for JSON events
#[derive(Debug)]
pub struct StdoutEmitter;

impl ProgressEmitter for StdoutEmitter {
    fn emit(&mut self, event: &ProgressEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        println!("{}", line);
        Ok(())
    }
}

/// Silent emitter that discards all events
#[derive(Debug)]
pub struct SilentEmitter;

impl ProgressEmitter for SilentEmitter {
    fn emit(&mut self, _event: &ProgressEvent) -> Result<()> {
        Ok(())
    }
}

/// Helper for tracking phase durations
#[derive(Debug)]
pub struct PhaseTracker {
    tracker: Arc<Mutex<Option<ProgressTracker>>>,
    operation: String,
    start_time: Instant,
}

impl PhaseTracker {
    /// Start tracking a phase
    pub fn new(tracker: Arc<Mutex<Option<ProgressTracker>>>, operation: String) -> Self {
        Self {
            tracker,
            operation,
            start_time: Instant::now(),
        }
    }

    /// Complete the phase and record duration
    pub fn complete(self) {
        let duration = self.start_time.elapsed();
        if let Ok(tracker_guard) = self.tracker.lock() {
            if let Some(tracker) = tracker_guard.as_ref() {
                tracker.record_duration(&self.operation, duration);
            }
        }
    }
}

impl Drop for PhaseTracker {
    fn drop(&mut self) {
        let duration = self.start_time.elapsed();
        if let Ok(tracker_guard) = self.tracker.lock() {
            if let Some(tracker) = tracker_guard.as_ref() {
                tracker.record_duration(&self.operation, duration);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_event_id_generation() {
        let id1 = ProgressTracker::next_event_id();
        let id2 = ProgressTracker::next_event_id();
        assert!(id2 > id1);
    }

    #[test]
    fn test_event_serialization() {
        let event = ProgressEvent::BuildBegin {
            id: 1,
            timestamp: 1234567890,
            context: "test".to_string(),
            dockerfile: Some("Dockerfile".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("build.begin"));
        assert!(json.contains("test"));

        let deserialized: ProgressEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_histogram() {
        let mut histogram = DurationHistogram::new();

        histogram.record(Duration::from_millis(25));
        histogram.record(Duration::from_millis(75));
        histogram.record(Duration::from_secs(2));

        let summary = histogram.summary();
        assert_eq!(summary.count, 3);
        assert!(summary.avg_duration > Duration::from_millis(600));
    }

    #[test]
    fn test_metrics() {
        let mut metrics = Metrics::new();

        metrics.record_duration("build", Duration::from_secs(1));
        metrics.record_duration("build", Duration::from_secs(2));
        metrics.record_duration("container", Duration::from_millis(500));

        let summary = metrics.summary();
        assert_eq!(summary.histograms.len(), 2);
        assert!(summary.histograms.contains_key("build"));
        assert!(summary.histograms.contains_key("container"));
    }

    #[test]
    fn test_audit_log() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut audit_log = AuditLog::new(temp_dir.path(), 1024)?;

        let event = ProgressEvent::BuildBegin {
            id: 1,
            timestamp: ProgressTracker::current_timestamp(),
            context: "test".to_string(),
            dockerfile: None,
        };

        audit_log.log_event(&event)?;

        let log_content = std::fs::read_to_string(temp_dir.path().join("audit.jsonl"))?;
        assert!(log_content.contains("build.begin"));

        Ok(())
    }

    #[test]
    fn test_silent_emitter() -> Result<()> {
        let mut emitter = SilentEmitter;
        let event = ProgressEvent::BuildBegin {
            id: 1,
            timestamp: ProgressTracker::current_timestamp(),
            context: "test".to_string(),
            dockerfile: None,
        };

        // Should not fail
        emitter.emit(&event)?;
        Ok(())
    }
}

/// Get the default cache directory for deacon
pub fn get_cache_dir() -> Result<PathBuf> {
    let cache_dir = if let Some(home) = dirs::home_dir() {
        home.join(".deacon").join("cache")
    } else {
        PathBuf::from(".deacon").join("cache")
    };

    // Ensure cache directory exists
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}

/// Progress format for CLI configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressFormat {
    None,
    Json,
    Auto,
}

/// Create progress tracker based on CLI configuration
pub fn create_progress_tracker(
    format: &ProgressFormat,
    progress_file: Option<&Path>,
    _workspace_folder: Option<&Path>,
) -> Result<Option<ProgressTracker>> {
    let cache_dir = get_cache_dir()?;

    let emitter: Option<Box<dyn ProgressEmitter>> = match format {
        ProgressFormat::None => None,
        ProgressFormat::Json => {
            if let Some(file_path) = progress_file {
                Some(Box::new(JsonFileEmitter::new(file_path)?))
            } else {
                Some(Box::new(StdoutEmitter))
            }
        }
        ProgressFormat::Auto => {
            // In auto mode, if progress_file is specified, write JSON to file
            // If terminal is TTY, could also display spinner/text (future enhancement)
            if let Some(file_path) = progress_file {
                Some(Box::new(JsonFileEmitter::new(file_path)?))
            } else {
                // For now, silent in auto mode without file
                // TODO: Add TTY spinner/text output
                None
            }
        }
    };

    let tracker = ProgressTracker::new(emitter, Some(&cache_dir))?;
    Ok(Some(tracker))
}
