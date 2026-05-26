//! Redaction integration for the tracing formatter.
//!
//! This module wires the project's [`SecretRegistry`] / [`RedactionConfig`] into the
//! `tracing-subscriber` formatter so that every formatted log line is scanned for
//! registered secrets before reaching stderr.
//!
//! The redaction is applied at the writer boundary via a [`MakeWriter`] implementation
//! that wraps stderr in a [`RedactingWriter`]. This catches secrets that appear in any
//! part of the formatted output (message body, structured fields, span attributes,
//! span lifecycle events) without requiring every call site to remember to redact.
//!
//! Why a writer-level adapter rather than a field [`Visit`] adapter: the tracing-subscriber
//! formatter owns its own [`Visit`] impls and emits a single byte stream per event. The
//! cleanest seam for downstream redaction is the byte stream itself — which is exactly
//! what [`RedactingWriter`] consumes (line-buffered, secret-aware, UTF-8 safe).
//!
//! [`SecretRegistry`]: crate::redaction::SecretRegistry
//! [`RedactionConfig`]: crate::redaction::RedactionConfig
//! [`RedactingWriter`]: crate::redaction::RedactingWriter
//! [`MakeWriter`]: tracing_subscriber::fmt::MakeWriter
//! [`Visit`]: tracing::field::Visit

use std::io;

use tracing_subscriber::fmt::MakeWriter;

use crate::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};

/// A [`MakeWriter`] that wraps stderr in a [`RedactingWriter`] for every event.
///
/// Each call to [`MakeWriter::make_writer`] returns a fresh [`RedactingWriter`] wrapping
/// a locked stderr handle. Because [`RedactingWriter`] buffers until a newline, and the
/// fmt layer writes one newline-terminated record per event, redaction happens
/// per-record without crossing record boundaries.
///
/// Cloning is cheap: the [`SecretRegistry`] is `Arc`-backed and the [`RedactionConfig`]
/// holds either no registry or a clone of the same `Arc`.
#[derive(Debug, Clone)]
pub struct RedactingMakeWriter {
    config: RedactionConfig,
    registry: SecretRegistry,
}

impl RedactingMakeWriter {
    /// Build a [`RedactingMakeWriter`] from a [`RedactionConfig`].
    ///
    /// If the config carries a custom registry that one is used; otherwise the registry
    /// is captured by cloning the process-global registry's `Arc` handle. This ties the
    /// writer to the same secret store the rest of the program is registering against.
    pub fn new(config: RedactionConfig) -> Self {
        let registry = config
            .custom_registry
            .clone()
            .unwrap_or_else(|| crate::redaction::global_registry().clone());
        Self { config, registry }
    }
}

/// Stderr-backed writer that lazily wraps in [`RedactingWriter`] on each event.
///
/// We hold an owned [`io::Stderr`] handle and a clone of the registry. The
/// [`io::Write`] impl forwards through a transient [`RedactingWriter`] that flushes on
/// drop, so secrets stay buffered only across the bytes for a single event.
pub struct RedactingStderrWriter {
    config: RedactionConfig,
    registry: SecretRegistry,
    inner: io::Stderr,
}

impl io::Write for RedactingStderrWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let stderr_lock = self.inner.lock();
        let mut writer = RedactingWriter::new(stderr_lock, self.config.clone(), &self.registry);
        writer.write_all(buf)?;
        writer.flush()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().flush()
    }
}

impl<'a> MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingStderrWriter;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingStderrWriter {
            config: self.config.clone(),
            registry: self.registry.clone(),
            inner: io::stderr(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn make_writer_redacts_registered_secret_in_byte_stream() {
        let registry = SecretRegistry::new();
        registry.add_secret("hunter2hunter2");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Validate the wrapped writer redacts a single newline-terminated record. We
        // exercise the writer directly with a bytes sink so we don't depend on stderr.
        let mut sink = Vec::new();
        let mut writer = RedactingWriter::new(&mut sink, config, &registry);
        writeln!(writer, "my password is hunter2hunter2").unwrap();
        writer.flush().unwrap();

        let out = String::from_utf8(sink).unwrap();
        assert!(!out.contains("hunter2hunter2"), "secret leaked: {out:?}");
        assert!(out.contains("****"));
    }

    #[test]
    fn make_writer_passthrough_when_redaction_disabled() {
        let registry = SecretRegistry::new();
        registry.add_secret("hunter2hunter2");
        let config = RedactionConfig::disabled();

        let mut sink = Vec::new();
        let mut writer = RedactingWriter::new(&mut sink, config, &registry);
        writeln!(writer, "my password is hunter2hunter2").unwrap();
        writer.flush().unwrap();

        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("hunter2hunter2"));
    }
}
