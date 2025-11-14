//! Progress event streaming, metrics collection, and audit logging
//!
//! This module provides structured progress events for major operations,
//! in-memory metrics collection with histogram aggregation, and persistent
//! audit logging with rotation.

use anyhow::Result;
use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, instrument, warn};

use crate::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};

/// Global event ID counter for deterministic ordering
pub static EVENT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

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

    /// Lifecycle command events
    #[serde(rename = "lifecycle.command.begin")]
    LifecycleCommandBegin {
        id: u64,
        timestamp: u64,
        phase: String,
        command_id: String,
        command: String,
    },
    #[serde(rename = "lifecycle.command.end")]
    LifecycleCommandEnd {
        id: u64,
        timestamp: u64,
        phase: String,
        command_id: String,
        duration_ms: u64,
        success: bool,
        exit_code: Option<i32>,
    },

    /// Image scanning events
    #[serde(rename = "scan.begin")]
    ScanBegin {
        id: u64,
        timestamp: u64,
        image_id: String,
        command: String,
    },
    #[serde(rename = "scan.end")]
    ScanEnd {
        id: u64,
        timestamp: u64,
        image_id: String,
        duration_ms: u64,
        success: bool,
        exit_code: Option<i32>,
    },

    /// OCI registry operation events
    #[serde(rename = "oci.publish.begin")]
    OciPublishBegin {
        id: u64,
        timestamp: u64,
        registry: String,
        repository: String,
        tag: String,
    },
    #[serde(rename = "oci.publish.end")]
    OciPublishEnd {
        id: u64,
        timestamp: u64,
        registry: String,
        repository: String,
        tag: String,
        duration_ms: u64,
        success: bool,
        digest: Option<String>,
    },
    #[serde(rename = "oci.fetch.begin")]
    OciFetchBegin {
        id: u64,
        timestamp: u64,
        registry: String,
        repository: String,
        tag: String,
    },
    #[serde(rename = "oci.fetch.end")]
    OciFetchEnd {
        id: u64,
        timestamp: u64,
        registry: String,
        repository: String,
        tag: String,
        duration_ms: u64,
        success: bool,
        cached: bool,
    },
}

impl ProgressEvent {
    /// Returns the unique identifier for this event.
    ///
    /// Every `ProgressEvent` variant carries an `id` field; this method returns that id as a `u64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::ProgressEvent;
    /// let ev = ProgressEvent::BuildBegin { id: 42, timestamp: 0, context: "ctx".into(), dockerfile: None };
    /// assert_eq!(ev.id(), 42);
    /// ```
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
            ProgressEvent::LifecycleCommandBegin { id, .. } => *id,
            ProgressEvent::LifecycleCommandEnd { id, .. } => *id,
            ProgressEvent::ScanBegin { id, .. } => *id,
            ProgressEvent::ScanEnd { id, .. } => *id,
            ProgressEvent::OciPublishBegin { id, .. } => *id,
            ProgressEvent::OciPublishEnd { id, .. } => *id,
            ProgressEvent::OciFetchBegin { id, .. } => *id,
            ProgressEvent::OciFetchEnd { id, .. } => *id,
        }
    }

    /// Returns the event's timestamp in milliseconds since the Unix epoch.
    ///
    /// Every `ProgressEvent` variant carries a `timestamp` field; this accessor returns
    /// that value for the event.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::ProgressEvent;
    /// let ev = ProgressEvent::BuildBegin {
    ///     id: 1,
    ///     timestamp: 1_632_000_000,
    ///     context: "example".into(),
    ///     dockerfile: None,
    /// };
    /// assert_eq!(ev.timestamp(), 1_632_000_000);
    /// ```
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
            ProgressEvent::LifecycleCommandBegin { timestamp, .. } => *timestamp,
            ProgressEvent::LifecycleCommandEnd { timestamp, .. } => *timestamp,
            ProgressEvent::ScanBegin { timestamp, .. } => *timestamp,
            ProgressEvent::ScanEnd { timestamp, .. } => *timestamp,
            ProgressEvent::OciPublishBegin { timestamp, .. } => *timestamp,
            ProgressEvent::OciPublishEnd { timestamp, .. } => *timestamp,
            ProgressEvent::OciFetchBegin { timestamp, .. } => *timestamp,
            ProgressEvent::OciFetchEnd { timestamp, .. } => *timestamp,
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
    /// Constructs a new `DurationHistogram` with a sensible set of default buckets.
    ///
    /// The histogram uses the following bucket thresholds (ascending): 10ms, 50ms, 100ms, 500ms,
    /// 1s, 5s, 10s, 30s, 60s, 300s. An extra overflow bucket collects durations greater than the
    /// largest threshold. All counters and totals are initialized to zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::DurationHistogram;
    /// let hist = DurationHistogram::new();
    /// assert_eq!(hist.summary().count, 0);
    /// ```
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

    /// Records a measured duration into the histogram.
    ///
    /// Increments the histogram's total count and total duration, then assigns the duration
    /// to the first bucket whose threshold is >= `duration`. If the duration exceeds all
    /// configured bucket thresholds, it is counted in the overflow bucket (the final entry
    /// in `counts`).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use deacon_core::progress::DurationHistogram;
    /// let mut hist = DurationHistogram::new();
    /// hist.record(Duration::from_millis(50));
    /// let summary = hist.summary();
    /// assert_eq!(summary.count, 1);
    /// ```
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

    /// Returns a serializable summary of the histogram's recorded durations.
    ///
    /// The returned `HistogramSummary` contains:
    /// - `count`: total number of recorded samples.
    /// - `total_duration`: sum of all recorded `Duration`s.
    /// - `avg_duration`: average duration (zero if no samples).
    /// - `buckets`: per-bucket counts where each `threshold_ms` is the bucket boundary in milliseconds.
    /// - `overflow_count`: count of recordings that exceeded the largest bucket threshold.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use deacon_core::progress::DurationHistogram;
    ///
    /// // Create a histogram, record a single 150ms sample, and inspect the summary.
    /// let mut hist = DurationHistogram::new();
    /// hist.record(Duration::from_millis(150));
    /// let summary = hist.summary();
    /// assert_eq!(summary.count, 1);
    /// assert_eq!(summary.total_duration, Duration::from_millis(150));
    /// assert_eq!(summary.avg_duration, Duration::from_millis(150));
    /// // buckets and overflow_count reflect where 150ms falls relative to histogram thresholds
    /// ```
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
    /// Returns the default instance by delegating to `Self::new()`.
    ///
    /// # Examples
    ///
    /// ```
    /// // Obtain the default value for the type implementing `Default`.
    /// let _ = deacon_core::progress::DurationHistogram::default();
    /// ```
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for a histogram
/// Summary of a duration histogram containing aggregated statistics
///
/// Contains the total number of recorded events, total and average duration,
/// per-bucket counts, and overflow statistics for events exceeding the largest bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramSummary {
    /// Total number of events recorded
    pub count: u64,
    /// Sum of all recorded durations
    pub total_duration: Duration,
    /// Average duration per event
    pub avg_duration: Duration,
    /// Per-bucket statistics
    pub buckets: Vec<BucketSummary>,
    /// Number of events that exceeded the largest bucket threshold
    pub overflow_count: u64,
}

/// Summary for a single histogram bucket containing threshold and count
///
/// Each bucket represents events with duration less than or equal to the threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketSummary {
    /// Maximum duration (in milliseconds) for events in this bucket
    pub threshold_ms: u64,
    /// Number of events that fall within this bucket
    pub count: u64,
}

/// In-memory metrics collection
#[derive(Debug)]
pub struct Metrics {
    histograms: HashMap<String, DurationHistogram>,
}

impl Metrics {
    /// Create a new, empty Metrics collection.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use deacon_core::progress::Metrics;
    /// let mut metrics = Metrics::new();
    /// metrics.record_duration("compile", Duration::from_millis(15));
    /// let summary = metrics.summary();
    /// assert!(summary.histograms.contains_key("compile"));
    /// ```
    pub fn new() -> Self {
        Self {
            histograms: HashMap::new(),
        }
    }

    /// Record a duration for the named operation into the in-memory histogram.
    ///
    /// If a histogram for `operation` does not yet exist, one is created. The provided
    /// `duration` is added to that histogram's buckets and contributes to the overall
    /// totals.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use deacon_core::progress::Metrics;
    /// let mut metrics = Metrics::new();
    /// metrics.record_duration("build", Duration::from_millis(120));
    /// let summary = metrics.summary();
    /// assert!(summary.histograms.contains_key("build"));
    /// ```
    pub fn record_duration(&mut self, operation: &str, duration: Duration) {
        let histogram = self.histograms.entry(operation.to_string()).or_default();
        histogram.record(duration);
        debug!("Recorded {} duration: {:?}", operation, duration);
    }

    /// Returns a snapshot summary of all recorded histograms.
    ///
    /// Produces a MetricsSummary that maps each operation name to its corresponding
    /// HistogramSummary. This does not modify the metrics collector; it only
    /// aggregates the current state into a serializable summary.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use deacon_core::progress::Metrics;
    /// let mut metrics = Metrics::new();
    /// metrics.record_duration("build", Duration::from_millis(120));
    /// metrics.record_duration("build", Duration::from_millis(80));
    /// let summary = metrics.summary();
    /// assert!(summary.histograms.contains_key("build"));
    /// let hs = &summary.histograms["build"];
    /// assert_eq!(hs.count, 2);
    /// ```
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
    /// Returns the default instance by delegating to `Self::new()`.
    ///
    /// # Examples
    ///
    /// ```
    /// // Obtain the default value for the type implementing `Default`.
    /// let _ = deacon_core::progress::Metrics::default();
    /// ```
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of all collected metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub histograms: HashMap<String, HistogramSummary>,
}

/// Audit log writer with file rotation and redaction support
#[derive(Debug)]
pub struct AuditLog {
    file: BufWriter<File>,
    current_size: u64,
    max_size: u64,
    log_path: PathBuf,
    rotation_count: u32,
    redaction_config: crate::redaction::RedactionConfig,
}

impl AuditLog {
    /// Creates a new audit log backed by `cache_dir/audit.jsonl`, prepared for size-based rotation.
    ///
    /// Ensures the cache directory exists, opens (or creates) `audit.jsonl` in append mode, and
    /// initializes the writer and current file size. The returned AuditLog will rotate the file
    /// once written bytes reach `max_size`.
    ///
    /// # Parameters
    /// - `cache_dir`: directory under which `audit.jsonl` will be created (parent directories are created if missing).
    /// - `max_size`: rotation threshold in bytes; when `current_size` reaches or exceeds this value the log will be rotated.
    /// - `redaction_config`: configuration for redacting sensitive information from log entries.
    ///
    /// # Returns
    /// Returns `Ok(AuditLog)` on success or an I/O error if directory creation or file operations fail.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use deacon_core::progress::AuditLog;
    /// use deacon_core::redaction::RedactionConfig;
    /// let cache_dir = Path::new("/tmp");
    /// let redaction_config = RedactionConfig::default();
    /// let mut audit = AuditLog::new(cache_dir, 1024 * 1024, redaction_config).unwrap(); // 1 MiB rotation threshold
    /// ```
    pub fn new(
        cache_dir: &Path,
        max_size: u64,
        redaction_config: crate::redaction::RedactionConfig,
    ) -> Result<Self> {
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
            redaction_config,
        })
    }

    /// Appends a ProgressEvent to the audit log as a single JSON line, flushes the file,
    /// updates the tracked file size, and rotates the log file if the size threshold is reached.
    ///
    /// The event is serialized with `serde_json::to_string` and written followed by a newline.
    /// `current_size` is incremented by the serialized byte length plus one (for the newline).
    /// If the updated `current_size` is greater than or equal to `max_size`, the log is rotated
    /// via `rotate()`.
    ///
    /// # Errors
    ///
    /// Returns any I/O or serialization errors produced while serializing, writing, flushing,
    /// or performing rotation.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use deacon_core::progress::{AuditLog, ProgressEvent};
    /// use deacon_core::redaction::RedactionConfig;
    ///
    /// // Create an AuditLog in a temp directory (error handling omitted for brevity)
    /// let tmp = tempfile::tempdir().unwrap();
    /// let redaction_config = RedactionConfig::default();
    /// let mut audit = AuditLog::new(tmp.path(), 1024, redaction_config).unwrap();
    ///
    /// let event = ProgressEvent::BuildBegin {
    ///     id: 1,
    ///     timestamp: 0,
    ///     context: "ctx".into(),
    ///     dockerfile: None,
    /// };
    ///
    /// audit.log_event(&event).unwrap();
    /// ```
    #[instrument(skip(self))]
    pub fn log_event(&mut self, event: &ProgressEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        // Apply redaction to the serialized event
        let redacted_line = crate::redaction::redact_if_enabled(&line, &self.redaction_config);
        writeln!(self.file, "{}", redacted_line)?;
        self.file.flush()?;

        self.current_size += redacted_line.len() as u64 + 1; // +1 for newline
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

    /// Rotate the audit log file.
    ///
    /// This flushes and closes the current log writer, renames the active log file
    /// (`audit.jsonl`) to `audit.jsonl.{rotation_count}`, creates a new empty
    /// `audit.jsonl`, resets the in-memory current size counter to zero, and
    /// increments the rotation counter.
    ///
    /// Returns any I/O or serialization error encountered while flushing, renaming,
    /// or creating the new file.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// use deacon_core::progress::AuditLog;
    /// // `AuditLog::new` constructs an AuditLog pointing at `<cache_dir>/audit.jsonl`.
    /// let mut log = AuditLog::new(Path::new("/tmp"), 1024 * 1024).unwrap();
    /// // Force a rotation (e.g., after reaching max size)
    /// // Note: rotate is a private method; this example is for illustration only.
    /// // log.rotate().unwrap();
    /// ```
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
    /// Creates a new ProgressTracker.
    ///
    /// If `cache_dir` is `Some`, an AuditLog is created at `<cache_dir>/audit.jsonl` with a 10 MiB
    /// rotation threshold; otherwise no audit logging is enabled. Passing `None` for `emitter`
    /// disables external event emission (events will still be recorded to metrics and the audit log
    /// if present). The `redaction_config` controls how sensitive information is handled in audit logs.
    ///
    /// Returns an error if constructing the AuditLog fails (propagates IO/serialization errors).
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::ProgressTracker;
    /// use deacon_core::redaction::RedactionConfig;
    /// // Create a tracker with no emitter and no audit log.
    /// let redaction_config = RedactionConfig::default();
    /// let tracker = ProgressTracker::new(None, None, redaction_config).unwrap();
    /// ```
    pub fn new(
        emitter: Option<Box<dyn ProgressEmitter>>,
        cache_dir: Option<&Path>,
        redaction_config: crate::redaction::RedactionConfig,
    ) -> Result<Self> {
        let audit_log = if let Some(cache_dir) = cache_dir {
            Some(AuditLog::new(
                cache_dir,
                10 * 1024 * 1024,
                redaction_config,
            )?) // 10MB max size
        } else {
            None
        };

        Ok(Self {
            emitter,
            audit_log,
            metrics: Arc::new(Mutex::new(Metrics::new())),
        })
    }

    /// Dispatches a progress event to the configured emitter and the audit log.
    ///
    /// The event is forwarded to the optional `ProgressEmitter` (if configured) and
    /// appended to the `AuditLog` (if configured). Returns an error if either
    /// emission or audit logging fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::path::Path;
    /// # use deacon_core::progress::{ProgressTracker, ProgressEvent, SilentEmitter};
    /// # use deacon_core::redaction::RedactionConfig;
    /// // Construct a tracker with a silent emitter and no audit log.
    /// let redaction_config = RedactionConfig::default();
    /// let mut tracker = ProgressTracker::new(Some(Box::new(SilentEmitter {})), None, redaction_config).unwrap();
    ///
    /// let event = ProgressEvent::BuildBegin {
    ///     id: ProgressTracker::next_event_id(),
    ///     timestamp: ProgressTracker::current_timestamp(),
    ///     context: "example".into(),
    ///     dockerfile: None,
    /// };
    ///
    /// // Emit the event; with a SilentEmitter this should succeed and be a no-op.
    /// tracker.emit_event(event).unwrap();
    /// ```
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

    /// Records a duration for an operation into the tracker's in-memory metrics.
    ///
    /// The `operation` string is used as the histogram key; the duration is added to
    /// that operation's histogram (created if absent). If the internal metrics
    /// mutex cannot be acquired (for example, if it is poisoned), this call is a
    /// no-op.
    pub fn record_duration(&self, operation: &str, duration: Duration) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.record_duration(operation, duration);
        }
    }

    /// Returns a snapshot summary of recorded metrics.
    ///
    /// The returned `MetricsSummary` contains per-operation histogram summaries aggregated
    /// at the time of the call. Returns `None` if the internal metrics mutex cannot be
    /// locked (e.g., if it is poisoned).
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::ProgressTracker;
    /// use deacon_core::redaction::RedactionConfig;
    /// let redaction_config = RedactionConfig::default();
    /// let tracker = ProgressTracker::new(None, None, redaction_config).unwrap();
    /// if let Some(summary) = tracker.metrics_summary() {
    ///     for (op, hist) in summary.histograms.iter() {
    ///         println!("operation: {}, count: {}", op, hist.count);
    ///     }
    /// }
    /// ```
    pub fn metrics_summary(&self) -> Option<MetricsSummary> {
        self.metrics.lock().ok().map(|metrics| metrics.summary())
    }

    /// Returns a unique event identifier and advances the global counter.
    ///
    /// This atomically reads and increments the global `EVENT_ID_COUNTER` using
    /// sequentially consistent ordering, ensuring uniqueness across threads. The
    /// function returns the identifier value before the increment (i.e., the
    /// current counter value) and increments the counter for subsequent calls.
    ///
    /// # Examples
    ///
    /// ```
    /// let a = deacon_core::progress::ProgressTracker::next_event_id();
    /// let b = deacon_core::progress::ProgressTracker::next_event_id();
    /// assert!(b > a);
    /// ```
    pub fn next_event_id() -> u64 {
        EVENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    /// Returns the current wall-clock time as milliseconds since the Unix epoch.
    ///
    /// If the system clock is earlier than the Unix epoch, this function returns 0.
    ///
    /// # Examples
    ///
    /// ```
    /// let ts = deacon_core::progress::ProgressTracker::current_timestamp();
    /// // timestamp is expressed in milliseconds since 1970-01-01T00:00:00Z
    /// assert!(ts >= 0);
    /// ```
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
    /// Create a new JsonFileEmitter that writes JSONL events to `file_path`.
    ///
    /// Ensures the parent directory exists, opens (or creates) the file in append mode,
    /// and returns a buffered writer wrapped in a `JsonFileEmitter`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use deacon_core::progress::JsonFileEmitter;
    /// let emitter = JsonFileEmitter::new(Path::new("/tmp/myapp/progress.jsonl")).unwrap();
    /// // `emitter` can now be used to emit JSONL progress events.
    /// ```
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
    /// Writes a ProgressEvent as a single JSON line to the emitter's file and flushes the writer.
    ///
    /// The event is serialized to JSON, written followed by a newline, and the underlying writer is flushed
    /// before returning. Any serialization or I/O error is propagated via the returned `Result`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::path::PathBuf;
    /// # use std::fs;
    /// # use deacon_core::progress::{JsonFileEmitter, ProgressEvent, ProgressEmitter};
    /// // create a temporary file path in the system temp dir
    /// let mut path = std::env::temp_dir();
    /// path.push("deacon_progress_example.jsonl");
    /// let _ = fs::remove_file(&path); // ignore error if not present
    ///
    /// let mut emitter = JsonFileEmitter::new(&path).expect("create emitter");
    /// let event = ProgressEvent::BuildBegin {
    ///     id: 1,
    ///     timestamp: 0,
    ///     context: "ctx".into(),
    ///     dockerfile: None,
    /// };
    /// emitter.emit(&event).expect("emit event");
    /// ```
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
    /// Emits a ProgressEvent to standard output as a single JSON line.
    ///
    /// Serializes `event` to JSON and writes it to stdout followed by a newline.
    /// Returns any serialization error encountered.
    ///
    /// # Examples
    ///
    /// ```
    /// use anyhow::Result;
    /// use deacon_core::progress::{StdoutEmitter, ProgressEvent, ProgressEmitter};
    /// let mut emitter = StdoutEmitter;
    /// let event = ProgressEvent::BuildBegin {
    ///     id: 1,
    ///     timestamp: 0,
    ///     context: "ctx".into(),
    ///     dockerfile: None,
    /// };
    /// let res: Result<()> = emitter.emit(&event);
    /// assert!(res.is_ok());
    /// ```
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
    /// Silently discards a progress event.
    ///
    /// This implementation performs no action and always returns `Ok(())`.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::{SilentEmitter, ProgressEvent, ProgressEmitter};
    /// let mut emitter = SilentEmitter;
    /// let event = ProgressEvent::BuildBegin {
    ///     id: 1,
    ///     timestamp: 0,
    ///     context: "example".into(),
    ///     dockerfile: None,
    /// };
    /// assert!(emitter.emit(&event).is_ok());
    /// ```
    fn emit(&mut self, _event: &ProgressEvent) -> Result<()> {
        Ok(())
    }
}

/// Redacting JSON file emitter that writes to a file with secrets redacted
#[derive(Debug)]
pub struct RedactingJsonFileEmitter {
    writer: RedactingWriter<BufWriter<File>>,
}

impl RedactingJsonFileEmitter {
    /// Create a new RedactingJsonFileEmitter that writes redacted JSONL events to `file_path`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use deacon_core::progress::RedactingJsonFileEmitter;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let emitter = RedactingJsonFileEmitter::new(
    ///     Path::new("/tmp/myapp/progress.jsonl"),
    ///     config,
    ///     &registry
    /// ).unwrap();
    /// ```
    pub fn new(
        file_path: &Path,
        config: RedactionConfig,
        registry: &SecretRegistry,
    ) -> Result<Self> {
        // Ensure the parent directory exists
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;

        let buf_writer = BufWriter::new(file);
        let redacting_writer = RedactingWriter::new(buf_writer, config, registry);

        Ok(Self {
            writer: redacting_writer,
        })
    }
}

impl ProgressEmitter for RedactingJsonFileEmitter {
    /// Writes a ProgressEvent as a redacted JSON line to the emitter's file.
    fn emit(&mut self, event: &ProgressEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        self.writer.write_line(&line)?;
        Ok(())
    }
}

/// Redacting standard output emitter for JSON events
#[derive(Debug)]
pub struct RedactingStdoutEmitter {
    writer: RedactingWriter<std::io::Stdout>,
}

impl RedactingStdoutEmitter {
    /// Create a new RedactingStdoutEmitter
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::progress::RedactingStdoutEmitter;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let emitter = RedactingStdoutEmitter::new(config, &registry);
    /// ```
    pub fn new(config: RedactionConfig, registry: &SecretRegistry) -> Self {
        let stdout = std::io::stdout();
        let redacting_writer = RedactingWriter::new(stdout, config, registry);

        Self {
            writer: redacting_writer,
        }
    }
}

impl ProgressEmitter for RedactingStdoutEmitter {
    /// Emits a ProgressEvent to standard output as a redacted JSON line.
    fn emit(&mut self, event: &ProgressEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        self.writer.write_line(&line)?;
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
    /// Creates a new PhaseTracker that records the start time of an operation.
    ///
    /// The returned tracker captures the current instant and will record the elapsed
    /// duration to the associated `ProgressTracker` (if any) when completed or when
    /// it is dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::{Arc, Mutex};
    /// use deacon_core::progress::{PhaseTracker, ProgressTracker};
    /// // Create an optional ProgressTracker placeholder (none for this example).
    /// let tracker: Arc<Mutex<Option<ProgressTracker>>> = Arc::new(Mutex::new(None));
    /// let phase = PhaseTracker::new(tracker, "install".to_string());
    /// // When `phase` is dropped or `phase.complete()` is called, the elapsed duration
    /// // will be recorded to the contained ProgressTracker if present.
    /// ```
    pub fn new(tracker: Arc<Mutex<Option<ProgressTracker>>>, operation: String) -> Self {
        Self {
            tracker,
            operation,
            start_time: Instant::now(),
        }
    }

    /// Completes the tracked phase and records its elapsed duration into the associated tracker.
    ///
    /// This consumes the PhaseTracker, computes the elapsed time since it was created, and — if the
    /// underlying shared tracker is present — records the duration under the tracker’s operation name.
    /// Failures to acquire the internal mutex or a missing tracker are silently ignored.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::{Arc, Mutex};
    /// use deacon_core::progress::{PhaseTracker, ProgressTracker};
    /// // Create a PhaseTracker that doesn't record anywhere (None) — calling `complete` is still valid.
    /// let shared: Arc<Mutex<Option<ProgressTracker>>> = Arc::new(Mutex::new(None));
    /// let phase = PhaseTracker::new(shared, "build".to_string());
    /// // Consumes `phase`, computes elapsed time, and (if a tracker existed) would record it.
    /// phase.complete();
    /// ```
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
    /// Records the elapsed time for this phase into the associated `ProgressTracker` when the `PhaseTracker` is dropped.
    ///
    /// If a `ProgressTracker` is available inside the shared `Arc<Mutex<Option<ProgressTracker>>>` it will record the elapsed
    /// duration for the operation name held by this tracker. If no tracker is present or the mutex cannot be locked, this
    /// drop handler is a no-op.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::{Arc, Mutex};
    /// use deacon_core::progress::{PhaseTracker, ProgressTracker};
    ///
    /// // `tracker` may hold `Some(ProgressTracker)` or `None`. On drop, `PhaseTracker` records the duration
    /// // only if a `ProgressTracker` is present.
    /// let tracker: Arc<Mutex<Option<ProgressTracker>>> = Arc::new(Mutex::new(None));
    /// {
    ///     let _phase = PhaseTracker::new(tracker.clone(), "build".to_string());
    /// } // `_phase` is dropped here; duration is recorded if a ProgressTracker was set
    /// ```
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
    fn test_lifecycle_command_event_serialization() {
        let begin_event = ProgressEvent::LifecycleCommandBegin {
            id: 1,
            timestamp: 1234567890,
            phase: "postCreate".to_string(),
            command_id: "postCreate-1".to_string(),
            command: "echo 'hello'".to_string(),
        };

        let json = serde_json::to_string(&begin_event).unwrap();
        assert!(json.contains("lifecycle.command.begin"));
        assert!(json.contains("postCreate"));
        assert!(json.contains("postCreate-1"));
        assert!(json.contains("echo 'hello'"));

        let deserialized: ProgressEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(begin_event, deserialized);

        let end_event = ProgressEvent::LifecycleCommandEnd {
            id: 2,
            timestamp: 1234567891,
            phase: "postCreate".to_string(),
            command_id: "postCreate-1".to_string(),
            duration_ms: 500,
            success: true,
            exit_code: Some(0),
        };

        let json = serde_json::to_string(&end_event).unwrap();
        assert!(json.contains("lifecycle.command.end"));
        assert!(json.contains("postCreate"));
        assert!(json.contains("postCreate-1"));
        assert!(json.contains("500"));
        assert!(json.contains("true"));

        let deserialized: ProgressEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(end_event, deserialized);
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
        let redaction_config = crate::redaction::RedactionConfig::default();
        let mut audit_log = AuditLog::new(temp_dir.path(), 1024, redaction_config)?;

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

    #[test]
    fn test_redacting_stdout_emitter() -> Result<()> {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Note: Can't easily test stdout redaction in unit tests since it writes to stdout
        // This test mainly verifies the emitter can be created and doesn't panic
        let mut emitter = RedactingStdoutEmitter::new(config, &registry);

        let event = ProgressEvent::LifecycleCommandBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: "postCreate".to_string(),
            command_id: "cmd-1".to_string(),
            command: "echo secret123".to_string(),
        };

        // Should not fail
        emitter.emit(&event)?;
        Ok(())
    }

    #[test]
    fn test_redacting_json_file_emitter() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("progress.jsonl");

        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut emitter = RedactingJsonFileEmitter::new(&file_path, config, &registry)?;

        let event = ProgressEvent::LifecycleCommandBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: "postCreate".to_string(),
            command_id: "cmd-1".to_string(),
            command: "echo secret123 in command".to_string(),
        };

        emitter.emit(&event)?;
        drop(emitter); // Ensure file is flushed and closed

        // Read the file and verify redaction occurred
        let content = std::fs::read_to_string(&file_path)?;
        assert!(
            content.contains("****"),
            "Secret should be redacted in file output"
        );
        assert!(
            !content.contains("secret123"),
            "Original secret should not appear in file"
        );
        assert!(content.contains("echo"), "Non-secret content should remain");

        Ok(())
    }

    #[test]
    fn test_redacting_json_file_emitter_disabled() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("progress.jsonl");

        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::disabled();

        let mut emitter = RedactingJsonFileEmitter::new(&file_path, config, &registry)?;

        let event = ProgressEvent::LifecycleCommandBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: "postCreate".to_string(),
            command_id: "cmd-1".to_string(),
            command: "echo secret123 in command".to_string(),
        };

        emitter.emit(&event)?;
        drop(emitter); // Ensure file is flushed and closed

        // Read the file and verify redaction was disabled
        let content = std::fs::read_to_string(&file_path)?;
        assert!(
            content.contains("secret123"),
            "Secret should not be redacted when disabled"
        );
        assert!(
            !content.contains("****"),
            "Redaction placeholder should not appear"
        );

        Ok(())
    }

    #[test]
    fn test_create_progress_tracker_with_redaction() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("progress.jsonl");

        let registry = SecretRegistry::new();
        registry.add_secret("tracker_secret");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let tracker = create_progress_tracker(
            &ProgressFormat::Json,
            Some(&file_path),
            None,
            &config,
            &registry,
        )?;

        assert!(tracker.is_some());
        Ok(())
    }
}

/// Returns the default cache directory for Deacon, creating it if necessary.
///
/// Resolution order (first match wins):
/// - Environment override `DEACON_CACHE_DIR`
/// - `./.deacon/cache` relative to the current working directory
///
/// The directory is created with `create_dir_all` if it does not already exist.
///
/// # Errors
///
/// Returns an `Err` if the directory cannot be created or if filesystem operations fail.
///
/// # Examples
///
/// ```
/// let dir = deacon_core::progress::get_cache_dir().expect("failed to get cache dir");
/// assert!(!dir.as_os_str().is_empty());
/// ```
pub fn get_cache_dir() -> Result<PathBuf> {
    // Environment override to support hermetic/test-friendly operation
    if let Ok(dir) = std::env::var("DEACON_CACHE_DIR") {
        let path = PathBuf::from(dir);
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }
        return Ok(path);
    }

    // Default to a project-local cache directory to avoid writing outside the workspace
    let cache_dir = PathBuf::from(".deacon").join("cache");
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)?;
    }

    // Check for legacy system cache directory and warn user about the migration
    if let Some(proj_dirs) = ProjectDirs::from("com", "deacon", "deacon") {
        let old_cache_dir = proj_dirs.cache_dir();
        if old_cache_dir.exists() {
            warn!(
                "Found existing cache at {}. Deacon now uses project-local cache at {}. \
                 Consider migrating or removing the old cache directory.",
                old_cache_dir.display(),
                cache_dir.display()
            );
        }
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

/// Create a ProgressTracker configured from CLI options.
///
/// The chosen `ProgressFormat` and optional `progress_file` determine which
/// `ProgressEmitter` (if any) is attached:
/// - `ProgressFormat::None` -> no emitter (silent).
/// - `ProgressFormat::Json` -> writes JSON lines to `progress_file` if provided,
///   otherwise to stdout.
/// - `ProgressFormat::Auto` -> writes JSON lines to `progress_file` if provided;
///   otherwise remains silent (TTY-based progress UI may be added later).
///
/// The function also acquires or creates the default cache directory via
/// `get_cache_dir()` and uses it when constructing the tracker. If `progress_file`
/// points to a path that cannot be created or opened, or if any I/O/serialization
/// error occurs while building the emitter or audit log, an error is returned.
///
/// The `_workspace_folder` parameter is currently unused and reserved for future use.
///
/// # Examples
///
/// ```
/// # use std::path::Path;
/// # use deacon_core::progress::{create_progress_tracker, ProgressFormat};
/// # use deacon_core::redaction::{RedactionConfig, SecretRegistry};
/// let config = RedactionConfig::default();
/// let registry = SecretRegistry::new();
/// let tracker = create_progress_tracker(&ProgressFormat::Json, Some(Path::new("progress.jsonl")), None, &config, &registry).unwrap();
/// assert!(tracker.is_some());
/// ```
pub fn create_progress_tracker(
    format: &ProgressFormat,
    progress_file: Option<&Path>,
    _workspace_folder: Option<&Path>,
    redaction_config: &RedactionConfig,
    registry: &SecretRegistry,
) -> Result<Option<ProgressTracker>> {
    let cache_dir = get_cache_dir()?;

    let emitter: Option<Box<dyn ProgressEmitter>> = match format {
        ProgressFormat::None => None,
        ProgressFormat::Json => {
            if let Some(file_path) = progress_file {
                Some(Box::new(RedactingJsonFileEmitter::new(
                    file_path,
                    redaction_config.clone(),
                    registry,
                )?))
            } else {
                Some(Box::new(RedactingStdoutEmitter::new(
                    redaction_config.clone(),
                    registry,
                )))
            }
        }
        ProgressFormat::Auto => {
            // In auto mode, if progress_file is specified, write JSON to file
            // If terminal is TTY, could also display spinner/text (future enhancement)
            if let Some(file_path) = progress_file {
                Some(Box::new(RedactingJsonFileEmitter::new(
                    file_path,
                    redaction_config.clone(),
                    registry,
                )?))
            } else {
                // For now, silent in auto mode without file
                // TODO: Add TTY spinner/text output
                None
            }
        }
    };

    let tracker = ProgressTracker::new(emitter, Some(&cache_dir), redaction_config.clone())?;
    Ok(Some(tracker))
}

/// Create a ProgressTracker without redaction (for backward compatibility).
///
/// This is a convenience wrapper around the main `create_progress_tracker` function
/// that uses a disabled redaction config.
///
/// # Examples
///
/// ```
/// # use std::path::Path;
/// # use deacon_core::progress::{create_progress_tracker_no_redaction, ProgressFormat};
/// let tracker = create_progress_tracker_no_redaction(&ProgressFormat::Json, Some(Path::new("progress.jsonl")), None).unwrap();
/// assert!(tracker.is_some());
/// ```
pub fn create_progress_tracker_no_redaction(
    format: &ProgressFormat,
    progress_file: Option<&Path>,
    workspace_folder: Option<&Path>,
) -> Result<Option<ProgressTracker>> {
    let redaction_config = RedactionConfig::disabled();
    let registry = SecretRegistry::new();
    create_progress_tracker(
        format,
        progress_file,
        workspace_folder,
        &redaction_config,
        &registry,
    )
}
