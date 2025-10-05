//! Observability utilities for standardized tracing spans and structured fields
//!
//! This module provides helper functions and constants for consistent tracing
//! across core workflows, implementing the canonical span taxonomy defined
//! in the CLI specification.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;
use tracing::{span, Span};

/// Canonical span names for core workflows
pub mod spans {
    pub const CONFIG_RESOLVE: &str = "config.resolve";
    pub const FEATURE_PLAN: &str = "feature.plan";
    pub const FEATURE_INSTALL: &str = "feature.install";
    pub const TEMPLATE_APPLY: &str = "template.apply";
    pub const CONTAINER_BUILD: &str = "container.build";
    pub const CONTAINER_CREATE: &str = "container.create";
    pub const LIFECYCLE_RUN: &str = "lifecycle.run";
    pub const REGISTRY_PULL: &str = "registry.pull";
    pub const REGISTRY_PUBLISH: &str = "registry.publish";
}

/// Common field names for structured logging
pub mod fields {
    pub const WORKSPACE_ID: &str = "workspace_id";
    pub const FEATURE_ID: &str = "feature_id";
    pub const TEMPLATE_ID: &str = "template_id";
    pub const CONTAINER_ID: &str = "container_id";
    pub const IMAGE_ID: &str = "image_id";
    pub const REF: &str = "ref";
    pub const DURATION_MS: &str = "duration_ms";
}

/// Generate a deterministic workspace ID from a path
///
/// This creates an 8-character hex hash from the canonical path,
/// computed only once per execution for performance.
pub fn workspace_id(workspace_path: &Path) -> String {
    use crate::workspace::resolve_workspace_root;

    // Use worktree-aware resolution to get the canonical workspace root
    let canonical_path = resolve_workspace_root(workspace_path).unwrap_or_else(|_| {
        workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf())
    });

    let mut hasher = DefaultHasher::new();
    canonical_path.hash(&mut hasher);
    let hash = hasher.finish();

    // Use first 8 characters for short, zero-padded hex
    let hex = format!("{:016x}", hash);
    hex[..8].to_string()
}

/// Start a span for configuration resolution workflow
pub fn config_resolve_span(workspace_path: &Path) -> Span {
    let workspace_id = workspace_id(workspace_path);

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::CONFIG_RESOLVE,
        duration_ms = tracing::field::Empty,
        workspace_id = %workspace_id
    )
}

/// Start a span for feature planning workflow
pub fn feature_plan_span(workspace_path: &Path) -> Span {
    let workspace_id = workspace_id(workspace_path);

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::FEATURE_PLAN,
        duration_ms = tracing::field::Empty,
        workspace_id = %workspace_id
    )
}

/// Start a span for feature installation workflow
pub fn feature_install_span(workspace_path: &Path, feature_id: &str) -> Span {
    let workspace_id = workspace_id(workspace_path);

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::FEATURE_INSTALL,
        duration_ms = tracing::field::Empty,
        workspace_id = %workspace_id,
        feature_id = %feature_id
    )
}

/// Start a span for template application workflow
pub fn template_apply_span(template_id: &str, workspace_path: Option<&Path>) -> Span {
    let workspace_id = workspace_path.map(workspace_id).unwrap_or_default();

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::TEMPLATE_APPLY,
        duration_ms = tracing::field::Empty,
        template_id = %template_id,
        workspace_id = %workspace_id
    )
}

/// Start a span for container build workflow
pub fn container_build_span(workspace_path: &Path, image_id: Option<&str>) -> Span {
    let workspace_id = workspace_id(workspace_path);

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::CONTAINER_BUILD,
        duration_ms = tracing::field::Empty,
        workspace_id = %workspace_id,
        image_id = image_id.unwrap_or("")
    )
}

/// Start a span for container creation workflow  
pub fn container_create_span(workspace_path: &Path, container_id: Option<&str>) -> Span {
    let workspace_id = workspace_id(workspace_path);

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::CONTAINER_CREATE,
        duration_ms = tracing::field::Empty,
        workspace_id = %workspace_id,
        container_id = container_id.unwrap_or("")
    )
}

/// Start a span for lifecycle command execution
pub fn lifecycle_run_span(workspace_path: &Path, command_type: &str) -> Span {
    let workspace_id = workspace_id(workspace_path);

    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::LIFECYCLE_RUN,
        duration_ms = tracing::field::Empty,
        workspace_id = %workspace_id,
        command_type = %command_type
    )
}

/// Start a span for registry pull operations
pub fn registry_pull_span(registry_ref: &str) -> Span {
    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::REGISTRY_PULL,
        duration_ms = tracing::field::Empty,
        r#ref = %registry_ref
    )
}

/// Start a span for registry publish operations
pub fn registry_publish_span(registry_ref: &str) -> Span {
    span!(
        target: "deacon_core::observability",
        tracing::Level::INFO,
        spans::REGISTRY_PUBLISH,
        duration_ms = tracing::field::Empty,
        r#ref = %registry_ref
    )
}

/// Helper for recording duration on span completion
pub struct TimedSpan {
    span: Span,
    start_time: Instant,
    // Keep the span entered for the lifetime of TimedSpan
    _entered: tracing::span::EnteredSpan,
}

impl TimedSpan {
    /// Create a new timed span from an existing span
    pub fn new(span: Span) -> Self {
        let entered = span.clone().entered();
        Self {
            span,
            start_time: Instant::now(),
            _entered: entered,
        }
    }

    /// Complete the span and record duration
    pub fn complete(self) {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        self.span.record(fields::DURATION_MS, duration_ms);
    }

    /// Get the underlying span for recording additional fields
    pub fn span(&self) -> &Span {
        &self.span
    }
}

/// Macro to create and enter a standardized span with automatic timing
#[macro_export]
macro_rules! timed_span {
    ($span_fn:expr) => {{
        let span = $span_fn;
        $crate::observability::TimedSpan::new(span)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_workspace_id_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        let id1 = workspace_id(path);
        let id2 = workspace_id(path);

        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 8);
    }

    #[test]
    fn test_workspace_id_different_paths() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let id1 = workspace_id(temp_dir1.path());
        let id2 = workspace_id(temp_dir2.path());

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_span_creation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Just test that span creation doesn't panic
        let _span = config_resolve_span(workspace_path);
        let _span = feature_install_span(workspace_path, "test-feature");

        // Test passes if we reach here without panic
    }

    #[test]
    fn test_timed_span() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let timed_span = TimedSpan::new(config_resolve_span(workspace_path));

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(10));

        timed_span.complete();
        // Test completes successfully if no panic occurs
    }
}
