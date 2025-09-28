//! Output handling with stdout/stderr separation contract
//!
//! This module provides centralized output helpers that enforce the CLI's
//! stdout/stderr separation contract. All commands should use these helpers
//! instead of direct `println!` to prevent accidental mixing of logs with
//! machine-readable output.

use crate::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};

/// Output helper that enforces stdout/stderr separation contract
///
/// This helper ensures that:
/// - JSON modes write only JSON to stdout
/// - Text modes write only user-facing results to stdout  
/// - All logs and diagnostics go to stderr via tracing
///
/// # Examples
///
/// ```
/// use deacon_core::io::Output;
/// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
/// use serde_json::json;
///
/// let config = RedactionConfig::default();
/// let registry = SecretRegistry::new();
/// let mut output = Output::new(config, &registry);
///
/// // JSON output
/// let data = json!({"status": "success", "count": 42});
/// output.write_json(&data).unwrap();
///
/// // Text output  
/// output.write_line("Build completed successfully!").unwrap();
/// ```
pub struct Output {
    writer: RedactingWriter<Box<dyn Write>>,
}

impl Output {
    /// Create a new Output helper with redaction support
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::io::Output;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let output = Output::new(config, &registry);
    /// ```
    pub fn new(config: RedactionConfig, registry: &SecretRegistry) -> Self {
        let stdout = Box::new(io::stdout()) as Box<dyn Write>;
        let writer = RedactingWriter::new(stdout, config, registry);

        Self { writer }
    }

    /// Write a JSON-serializable value to stdout
    ///
    /// This method is intended for JSON output modes. It serializes the value
    /// as pretty-printed JSON and writes it to stdout with a trailing newline.
    /// Redaction is applied to the JSON string before output.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::io::Output;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    /// use serde_json::json;
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let mut output = Output::new(config, &registry);
    ///
    /// let result = json!({
    ///     "status": "success",
    ///     "features": ["docker", "node"],
    ///     "count": 2
    /// });
    ///
    /// output.write_json(&result).unwrap();
    /// ```
    pub fn write_json<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let json_output = serde_json::to_string(value)?;
        self.writer.write_line(&json_output)?;
        Ok(())
    }

    /// Write a text line to stdout
    ///
    /// This method is intended for human-readable text output modes. It writes
    /// the provided text to stdout with a trailing newline. Redaction is applied
    /// to the text before output.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::io::Output;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let mut output = Output::new(config, &registry);
    ///
    /// output.write_line("Build completed successfully!").unwrap();
    /// output.write_line("Container ID: abc123def456").unwrap();
    /// ```
    pub fn write_line(&mut self, text: &str) -> Result<()> {
        self.writer.write_line(text)?;
        Ok(())
    }

    /// Write multiple text lines to stdout
    ///
    /// Convenience method for writing multiple lines at once.
    /// Each line is written with a trailing newline.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::io::Output;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let mut output = Output::new(config, &registry);
    ///
    /// let lines = vec![
    ///     "Configuration Loaded Successfully",
    ///     "========================",
    ///     "",
    ///     "Name: my-devcontainer",
    ///     "Image: mcr.microsoft.com/devcontainers/base:ubuntu",
    /// ];
    ///
    /// output.write_lines(&lines).unwrap();
    /// ```
    pub fn write_lines(&mut self, lines: &[&str]) -> Result<()> {
        for line in lines {
            self.write_line(line)?;
        }
        Ok(())
    }

    /// Flush any buffered output
    ///
    /// Ensures all buffered content is written to stdout immediately.
    /// This is automatically called when the Output is dropped, but can
    /// be called explicitly if needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::io::Output;
    /// use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    ///
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let mut output = Output::new(config, &registry);
    ///
    /// output.write_line("Important message").unwrap();
    /// output.flush().unwrap(); // Ensure it's written immediately
    /// ```
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // Ensure any buffered content is flushed on drop
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::SecretRegistry;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    // Mock writer for testing output capture
    #[derive(Clone)]
    struct MockWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl MockWriter {
        fn new() -> Self {
            Self {
                buffer: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_output(&self) -> String {
            let buffer = self.buffer.lock().unwrap();
            String::from_utf8(buffer.clone()).unwrap()
        }
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut buffer = self.buffer.lock().unwrap();
            buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn create_test_output() -> (Output, MockWriter) {
        let config = RedactionConfig::default();
        let registry = SecretRegistry::new();
        let mock_writer = MockWriter::new();
        let writer = RedactingWriter::new(
            Box::new(mock_writer.clone()) as Box<dyn Write>,
            config,
            &registry,
        );

        let output = Output { writer };
        (output, mock_writer)
    }

    #[test]
    fn test_write_json() {
        let (mut output, mock_writer) = create_test_output();

        let data = json!({
            "command": "test",
            "status": "success",
            "count": 42
        });

        output.write_json(&data).unwrap();

        let result = mock_writer.get_output();

        // Should be valid JSON with newline
        assert!(result.contains("\"command\":\"test\""));
        assert!(result.contains("\"status\":\"success\""));
        assert!(result.contains("\"count\":42"));
        assert!(result.ends_with('\n'));

        // Should be parseable as JSON
        let parsed: serde_json::Value = serde_json::from_str(result.trim()).unwrap();
        assert_eq!(parsed["status"], "success");
    }

    #[test]
    fn test_write_line() {
        let (mut output, mock_writer) = create_test_output();

        output.write_line("Build completed successfully!").unwrap();

        let result = mock_writer.get_output();
        assert_eq!(result, "Build completed successfully!\n");
    }

    #[test]
    fn test_write_lines() {
        let (mut output, mock_writer) = create_test_output();

        let lines = vec![
            "Configuration Summary",
            "====================",
            "",
            "Name: test-container",
        ];

        output.write_lines(&lines).unwrap();

        let result = mock_writer.get_output();
        let expected = "Configuration Summary\n====================\n\nName: test-container\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_redaction_in_json() {
        let config = RedactionConfig::default();
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");

        let mock_writer = MockWriter::new();
        let writer = RedactingWriter::new(
            Box::new(mock_writer.clone()) as Box<dyn Write>,
            config,
            &registry,
        );
        let mut output = Output { writer };

        let data = json!({
            "password": "secret123",
            "username": "testuser"
        });

        output.write_json(&data).unwrap();

        let result = mock_writer.get_output();

        // Secret should be redacted
        assert!(result.contains("****"));
        assert!(!result.contains("secret123"));
        assert!(result.contains("testuser")); // Non-secret should remain
    }

    #[test]
    fn test_redaction_in_text() {
        let config = RedactionConfig::default();
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");

        let mock_writer = MockWriter::new();
        let writer = RedactingWriter::new(
            Box::new(mock_writer.clone()) as Box<dyn Write>,
            config,
            &registry,
        );
        let mut output = Output { writer };

        output.write_line("Database password: secret123").unwrap();

        let result = mock_writer.get_output();

        // Secret should be redacted
        assert!(result.contains("****"));
        assert!(!result.contains("secret123"));
        assert!(result.contains("Database password:"));
    }

    #[test]
    fn test_mixed_output_modes() {
        let (mut output, mock_writer) = create_test_output();

        // Write some text lines
        output.write_line("Starting operation...").unwrap();

        // Write JSON
        let data = json!({"progress": 50});
        output.write_json(&data).unwrap();

        // Write more text
        output.write_line("Operation completed.").unwrap();

        let result = mock_writer.get_output();

        // Should contain all outputs in order
        assert!(result.contains("Starting operation...\n"));
        assert!(result.contains("\"progress\":50"));
        assert!(result.contains("Operation completed.\n"));
    }

    #[test]
    fn test_flush() {
        let (mut output, mock_writer) = create_test_output();

        output.write_line("Test message").unwrap();
        output.flush().unwrap();

        let result = mock_writer.get_output();
        assert_eq!(result, "Test message\n");
    }
}
