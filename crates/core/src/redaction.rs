//! Secret redaction and sensitive output filtering
//!
//! This module provides centralized redaction of secret values across logs, events,
//! doctor bundle, and errors. It maintains an in-memory set of secret keys and hashed
//! values for detection via naive substring scanning with length thresholds.
//!
//! References: subcommand-specs/*/SPEC.md "Security and Compliance"

use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::{Arc, RwLock};

/// Minimum length for a value to be considered for redaction
const MIN_REDACTION_LENGTH: usize = 8;

/// Replacement text for redacted secrets
const REDACTION_PLACEHOLDER: &str = "****";

/// Thread-safe registry for storing secrets that should be redacted
#[derive(Debug, Clone)]
pub struct SecretRegistry {
    /// Inner storage protected by RwLock for thread safety
    inner: Arc<RwLock<SecretRegistryInner>>,
}

#[derive(Debug, Default)]
struct SecretRegistryInner {
    /// Exact secret strings to redact
    exact_secrets: HashSet<String>,
    /// SHA-256 hashes of secrets for additional detection
    secret_hashes: HashSet<String>,
    /// Structured secrets with context information to reduce false positives
    structured_secrets: Vec<StructuredSecret>,
}

/// A structured secret with context information to reduce false positives
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredSecret {
    /// The secret value to redact
    value: String,
    /// Optional key/field name that provides context (e.g., "password", "token", "api_key")
    key: Option<String>,
    /// Optional pattern that must appear near the secret for it to be redacted
    context_pattern: Option<String>,
    /// Whether this secret should only be redacted when found in key-value pairs
    require_key_context: bool,
}

impl StructuredSecret {
    /// Create a new StructuredSecret with validation
    pub fn new(
        value: String,
        key: Option<String>,
        context_pattern: Option<String>,
        require_key_context: bool,
    ) -> Option<Self> {
        if value.len() < MIN_REDACTION_LENGTH {
            return None;
        }

        Some(Self {
            value,
            key,
            context_pattern,
            require_key_context,
        })
    }

    /// Get the secret value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Get the key context
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// Get the context pattern
    pub fn context_pattern(&self) -> Option<&str> {
        self.context_pattern.as_deref()
    }

    /// Check if key context is required
    pub fn require_key_context(&self) -> bool {
        self.require_key_context
    }
}

impl SecretRegistry {
    /// Create a new empty secret registry
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SecretRegistryInner::default())),
        }
    }

    /// Add a secret value to the registry
    ///
    /// The secret will be stored both as an exact string and as a SHA-256 hash
    /// for detection purposes. Only values meeting the minimum length threshold
    /// will be added.
    pub fn add_secret(&self, secret: &str) {
        if secret.len() < MIN_REDACTION_LENGTH {
            return;
        }

        if let Ok(mut inner) = self.inner.write() {
            inner.exact_secrets.insert(secret.to_string());

            // Add SHA-256 hash for additional detection
            let hash = sha256_hash(secret);
            inner.secret_hashes.insert(hash);
        }
    }

    /// Add multiple secrets to the registry
    pub fn add_secrets<I>(&self, secrets: I)
    where
        I: IntoIterator<Item = String>,
    {
        for secret in secrets {
            self.add_secret(&secret);
        }
    }

    /// Add a structured secret with contextual information
    ///
    /// This allows for more sophisticated redaction that can consider context
    /// to reduce false positives. For example, the word "secret" might only
    /// be redacted when it appears in a key-value context like "password=secret123".
    /// Add a structured secret with contextual information
    ///
    /// This allows for more sophisticated redaction that can consider context
    /// to reduce false positives. For example, the word "secret" might only
    /// be redacted when it appears in a key-value context like "password=secret".
    /// Returns true if the secret was added, false if it was invalid or duplicate.
    pub fn add_structured_secret(&self, structured_secret: StructuredSecret) -> bool {
        if let Ok(mut inner) = self.inner.write() {
            // Check if this structured secret already exists
            if !inner.structured_secrets.contains(&structured_secret) {
                inner.structured_secrets.push(structured_secret);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Add a secret with key context for key-value pair redaction
    ///
    /// This will only redact the secret when it appears after one of the specified keys.
    /// Useful for values that might appear in normal text but should only be redacted
    /// when they're actually secret values.
    /// Add a secret with key context for key-value pair redaction
    ///
    /// This will only redact the secret when it appears after one of the specified keys.
    /// Useful for values that might appear in normal text but should only be redacted
    /// when they're actually secret values.
    pub fn add_secret_with_key_context(&self, secret: &str, keys: Vec<String>) {
        for key in keys {
            if let Some(structured_secret) =
                StructuredSecret::new(secret.to_string(), Some(key), None, true)
            {
                self.add_structured_secret(structured_secret);
            }
        }
    }

    /// Check if a value contains any registered secrets and return redacted version
    ///
    /// This performs both simple substring scanning for exact secrets and their hashes,
    /// as well as contextual redaction for structured secrets.
    /// If any secrets are found, they are replaced with the redaction placeholder.
    pub fn redact_text(&self, text: &str) -> String {
        if let Ok(inner) = self.inner.read() {
            let mut result = text.to_string();

            // Redact exact secret matches
            for secret in &inner.exact_secrets {
                if result.contains(secret) {
                    result = result.replace(secret, REDACTION_PLACEHOLDER);
                }
            }

            // Redact hash matches (only if they meet minimum length)
            for hash in &inner.secret_hashes {
                if hash.len() >= MIN_REDACTION_LENGTH && result.contains(hash) {
                    result = result.replace(hash, REDACTION_PLACEHOLDER);
                }
            }

            // Redact structured secrets with context
            for structured_secret in &inner.structured_secrets {
                result = redact_structured_secret(&result, structured_secret);
            }

            result
        } else {
            // If we can't acquire the lock, return original text
            text.to_string()
        }
    }

    /// Get the count of registered secrets (for testing/debugging)
    pub fn secret_count(&self) -> usize {
        if let Ok(inner) = self.inner.read() {
            inner.exact_secrets.len() + inner.structured_secrets.len()
        } else {
            0
        }
    }

    /// Clear all registered secrets
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.exact_secrets.clear();
            inner.secret_hashes.clear();
            inner.structured_secrets.clear();
        }
    }
}

impl Default for SecretRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global secret registry instance
static GLOBAL_REGISTRY: once_cell::sync::Lazy<SecretRegistry> =
    once_cell::sync::Lazy::new(SecretRegistry::new);

/// Get the global secret registry instance
pub fn global_registry() -> &'static SecretRegistry {
    &GLOBAL_REGISTRY
}

/// Configuration for redaction behavior
#[derive(Debug, Clone)]
pub struct RedactionConfig {
    /// Whether redaction is enabled
    pub enabled: bool,
    /// Custom redaction placeholder (if different from default)
    pub placeholder: Option<String>,
    /// Custom registry to use instead of global (primarily for testing)
    pub custom_registry: Option<SecretRegistry>,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            placeholder: None,
            custom_registry: None,
        }
    }
}

impl RedactionConfig {
    /// Create config with redaction disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            placeholder: None,
            custom_registry: None,
        }
    }

    /// Create config with custom placeholder
    pub fn with_placeholder(placeholder: String) -> Self {
        Self {
            enabled: true,
            placeholder: Some(placeholder),
            custom_registry: None,
        }
    }

    /// Create config with custom registry (primarily for testing)
    pub fn with_custom_registry(registry: SecretRegistry) -> Self {
        Self {
            enabled: true,
            placeholder: None,
            custom_registry: Some(registry),
        }
    }

    /// Create config with both custom placeholder and registry
    pub fn with_placeholder_and_registry(placeholder: String, registry: SecretRegistry) -> Self {
        Self {
            enabled: true,
            placeholder: Some(placeholder),
            custom_registry: Some(registry),
        }
    }
}

/// Redact text using the global registry if redaction is enabled
pub fn redact_if_enabled(text: &str, config: &RedactionConfig) -> String {
    let registry = config
        .custom_registry
        .as_ref()
        .unwrap_or_else(|| global_registry());
    redact_with_registry(text, config, registry)
}

/// Redact text using a specific registry if redaction is enabled
pub fn redact_with_registry(
    text: &str,
    config: &RedactionConfig,
    registry: &SecretRegistry,
) -> String {
    if !config.enabled {
        return text.to_string();
    }

    let redacted = registry.redact_text(text);

    // Apply custom placeholder if specified
    if let Some(custom_placeholder) = &config.placeholder {
        redacted.replace(REDACTION_PLACEHOLDER, custom_placeholder)
    } else {
        redacted
    }
}

/// Redact a structured secret with contextual information
///
/// This function applies more sophisticated redaction logic based on the context
/// provided in the StructuredSecret. It can handle key-value pairs and context patterns.
fn redact_structured_secret(text: &str, structured_secret: &StructuredSecret) -> String {
    // If no special context is required, do simple replacement
    if !structured_secret.require_key_context() && structured_secret.context_pattern().is_none() {
        return text.replace(structured_secret.value(), REDACTION_PLACEHOLDER);
    }

    let mut result = text.to_string();

    // Handle key-value context redaction
    if structured_secret.require_key_context() {
        if let Some(key) = structured_secret.key() {
            // Look for patterns like "key=value", "key: value", "key": "value", etc.
            let patterns = [
                format!("{}={}", key, structured_secret.value()),
                format!("{}:{}", key, structured_secret.value()),
                format!("{}: {}", key, structured_secret.value()),
                format!("\"{}\":\"{}", key, structured_secret.value()),
                format!("\"{}\":\"{}\"", key, structured_secret.value()),
                format!("\"{}\" : \"{}\"", key, structured_secret.value()),
                format!("{}=\"{}\"", key, structured_secret.value()),
                format!("{} = \"{}\"", key, structured_secret.value()),
                format!("{} = {}", key, structured_secret.value()),
            ];

            for pattern in &patterns {
                if result.contains(pattern) {
                    let redacted_pattern =
                        pattern.replace(structured_secret.value(), REDACTION_PLACEHOLDER);
                    result = result.replace(pattern, &redacted_pattern);
                }
            }
        }
    }

    // Handle context pattern matching
    if let Some(context_pattern) = structured_secret.context_pattern() {
        // Only redact the secret if the context pattern is found nearby
        if result.contains(context_pattern) {
            result = result.replace(structured_secret.value(), REDACTION_PLACEHOLDER);
        }
    }

    result
}

/// Add a secret to the global registry
pub fn add_global_secret(secret: &str) {
    global_registry().add_secret(secret);
}

/// Add multiple secrets to the global registry
pub fn add_global_secrets<I>(secrets: I)
where
    I: IntoIterator<Item = String>,
{
    global_registry().add_secrets(secrets);
}

/// A writer that applies redaction to all output at line boundaries
///
/// `RedactingWriter` wraps any `Write`/`BufWrite` sink and applies redaction
/// line-by-line before forwarding output to the underlying writer. It buffers
/// partial lines until a newline is encountered to ensure secrets spanning
/// multiple write calls are properly redacted.
///
/// # Examples
///
/// ```
/// use std::io::Write;
/// use deacon_core::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
///
/// let registry = SecretRegistry::new();
/// registry.add_secret("my-secret-123");
/// let config = RedactionConfig::with_custom_registry(registry.clone());
///
/// let mut output = Vec::new();
/// let mut writer = RedactingWriter::new(&mut output, config, &registry);
///
/// write!(writer, "This contains my-secret-123 data\n").unwrap();
/// writer.flush().unwrap();
///
/// let result = String::from_utf8(output).unwrap();
/// assert_eq!(result, "This contains **** data\n");
/// ```
#[derive(Debug)]
pub struct RedactingWriter<W> {
    inner: W,
    buffer: Vec<u8>,
    config: RedactionConfig,
    registry: SecretRegistry,
}

impl<W: Write> RedactingWriter<W> {
    /// Create a new RedactingWriter
    ///
    /// # Arguments
    /// * `writer` - The underlying writer to forward redacted output to
    /// * `config` - Configuration controlling redaction behavior  
    /// * `registry` - Registry containing secrets to redact
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
    ///
    /// let mut output = Vec::new();
    /// let config = RedactionConfig::default();
    /// let registry = SecretRegistry::new();
    /// let writer = RedactingWriter::new(&mut output, config, &registry);
    /// ```
    pub fn new(writer: W, config: RedactionConfig, registry: &SecretRegistry) -> Self {
        Self {
            inner: writer,
            buffer: Vec::new(),
            config,
            registry: registry.clone(),
        }
    }

    /// Write a complete line with redaction applied
    ///
    /// This is a convenience method that adds a newline and immediately
    /// applies redaction and flushes the output.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
    ///
    /// let registry = SecretRegistry::new();
    /// registry.add_secret("secret123");
    /// let config = RedactionConfig::with_custom_registry(registry.clone());
    ///
    /// let mut output = Vec::new();
    /// let mut writer = RedactingWriter::new(&mut output, config, &registry);
    ///
    /// writer.write_line("This contains secret123 data").unwrap();
    ///
    /// let result = String::from_utf8(output).unwrap();
    /// assert_eq!(result, "This contains **** data\n");
    /// ```
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.write_all(line.as_bytes())?;
        self.write_all(b"\n")?;
        self.flush()
    }

    /// Process complete lines from the buffer
    fn process_complete_lines(&mut self) -> io::Result<()> {
        while let Some(newline_pos) = self.buffer.iter().position(|&b| b == b'\n') {
            // Extract line including newline
            let line_bytes: Vec<u8> = self.buffer.drain(..=newline_pos).collect();

            // Convert to string for redaction (best effort for non-UTF8)
            match String::from_utf8(line_bytes.clone()) {
                Ok(line_str) => {
                    // Apply redaction to the line
                    let redacted = redact_with_registry(&line_str, &self.config, &self.registry);
                    self.inner.write_all(redacted.as_bytes())?;
                }
                Err(_) => {
                    // If not valid UTF-8, pass through as-is
                    // TODO: Could do best-effort redaction on UTF-8 substrings
                    self.inner.write_all(&line_bytes)?;
                }
            }
        }
        Ok(())
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Add to buffer
        self.buffer.extend_from_slice(buf);

        // Process any complete lines
        self.process_complete_lines()?;

        // Return the full buffer size to indicate all bytes were "written"
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Process any remaining buffered content as a final line
        if !self.buffer.is_empty() {
            let remaining: Vec<u8> = self.buffer.drain(..).collect();

            match String::from_utf8(remaining.clone()) {
                Ok(remaining_str) => {
                    let redacted =
                        redact_with_registry(&remaining_str, &self.config, &self.registry);
                    self.inner.write_all(redacted.as_bytes())?;
                }
                Err(_) => {
                    self.inner.write_all(&remaining)?;
                }
            }
        }

        self.inner.flush()
    }
}

/// Cryptographic SHA-256 hash function for secure secret hashing
fn sha256_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_registry_creation() {
        let registry = SecretRegistry::new();
        assert_eq!(registry.secret_count(), 0);
    }

    #[test]
    fn test_add_secret() {
        let registry = SecretRegistry::new();
        registry.add_secret("my-secret-password");
        assert_eq!(registry.secret_count(), 1);
    }

    #[test]
    fn test_add_secret_too_short() {
        let registry = SecretRegistry::new();
        registry.add_secret("short"); // Only 5 characters
        assert_eq!(registry.secret_count(), 0);
    }

    #[test]
    fn test_redact_exact_match() {
        let registry = SecretRegistry::new();
        let secret = "my-secret-password";
        registry.add_secret(secret);

        let text = "The password is my-secret-password and should be hidden";
        let redacted = registry.redact_text(text);
        assert_eq!(redacted, "The password is **** and should be hidden");
    }

    #[test]
    fn test_redact_multiple_occurrences() {
        let registry = SecretRegistry::new();
        let secret = "secret123";
        registry.add_secret(secret);

        let text = "secret123 appears twice: secret123";
        let redacted = registry.redact_text(text);
        assert_eq!(redacted, "**** appears twice: ****");
    }

    #[test]
    fn test_redact_no_match() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");

        let text = "This text contains no secrets";
        let redacted = registry.redact_text(text);
        assert_eq!(redacted, text);
    }

    #[test]
    fn test_redact_partial_match_not_redacted() {
        let registry = SecretRegistry::new();
        registry.add_secret("password123");

        let text = "The word password appears but not the full secret";
        let redacted = registry.redact_text(text);
        assert_eq!(redacted, text); // Should not be redacted
    }

    #[test]
    fn test_add_multiple_secrets() {
        let registry = SecretRegistry::new();
        let secrets = vec![
            "secretone".to_string(),
            "secrettwo".to_string(),
            "password123".to_string(),
        ];
        registry.add_secrets(secrets);
        assert_eq!(registry.secret_count(), 3);
    }

    #[test]
    fn test_clear_secrets() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        assert_eq!(registry.secret_count(), 1);

        registry.clear();
        assert_eq!(registry.secret_count(), 0);
    }

    #[test]
    fn test_global_registry() {
        let registry = global_registry();
        let initial_count = registry.secret_count();

        add_global_secret("global-test-secret");
        assert_eq!(registry.secret_count(), initial_count + 1);

        // Clean up for other tests
        registry.clear();
    }

    #[test]
    fn test_redaction_config_default() {
        let config = RedactionConfig::default();
        assert!(config.enabled);
        assert!(config.placeholder.is_none());
    }

    #[test]
    fn test_redaction_config_disabled() {
        let config = RedactionConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_redaction_config_custom_placeholder() {
        let config = RedactionConfig::with_placeholder("[REDACTED]".to_string());
        assert!(config.enabled);
        assert_eq!(config.placeholder.as_ref().unwrap(), "[REDACTED]");
        assert!(config.custom_registry.is_none());
    }

    #[test]
    fn test_redaction_config_custom_registry() {
        let registry = SecretRegistry::new();
        let config = RedactionConfig::with_custom_registry(registry);
        assert!(config.enabled);
        assert!(config.placeholder.is_none());
        assert!(config.custom_registry.is_some());
    }

    #[test]
    fn test_redaction_config_custom_placeholder_and_registry() {
        let registry = SecretRegistry::new();
        let config =
            RedactionConfig::with_placeholder_and_registry("[HIDDEN]".to_string(), registry);
        assert!(config.enabled);
        assert_eq!(config.placeholder.as_ref().unwrap(), "[HIDDEN]");
        assert!(config.custom_registry.is_some());
    }

    #[test]
    fn test_redact_if_enabled_disabled() {
        let config = RedactionConfig::disabled();
        let text = "This contains secret123 but should not be redacted";

        // Even if we add the secret to global registry
        add_global_secret("secret123");

        let result = redact_if_enabled(text, &config);
        assert_eq!(result, text);

        // Clean up
        global_registry().clear();
    }

    #[test]
    fn test_redact_if_enabled_enabled() {
        let config = RedactionConfig::default();
        let text = "This contains secret123 and should be redacted";

        add_global_secret("secret123");

        let result = redact_if_enabled(text, &config);
        assert_eq!(result, "This contains **** and should be redacted");

        // Clean up
        global_registry().clear();
    }

    #[test]
    fn test_redact_if_enabled_custom_placeholder() {
        let config = RedactionConfig::with_placeholder("[HIDDEN]".to_string());
        let text = "This contains secret123 and should be redacted";

        add_global_secret("secret123");

        let result = redact_if_enabled(text, &config);
        assert_eq!(result, "This contains [HIDDEN] and should be redacted");

        // Clean up
        global_registry().clear();
    }

    #[test]
    fn test_redact_if_enabled_with_custom_registry() {
        let registry = SecretRegistry::new();
        registry.add_secret("test-secret-123");

        let config = RedactionConfig::with_custom_registry(registry);
        let text = "This contains test-secret-123";

        let result = redact_if_enabled(text, &config);
        assert_eq!(result, "This contains ****");
        assert!(!result.contains("test-secret-123"));

        // Verify global registry is unaffected
        let global_result = global_registry().redact_text(text);
        assert_eq!(global_result, text); // Should not be redacted by global registry
    }

    #[test]
    fn test_sha256_hash_function() {
        let input = "test-string";
        let hash1 = sha256_hash(input);
        let hash2 = sha256_hash(input);

        // Same input should produce same hash
        assert_eq!(hash1, hash2);

        // Different input should produce different hash
        let hash3 = sha256_hash("different-string");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_redacting_writer_enabled() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        writer.write_all(b"This contains secret123 data\n").unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "This contains **** data\n");
    }

    #[test]
    fn test_redacting_writer_disabled() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::disabled();

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        writer.write_all(b"This contains secret123 data\n").unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "This contains secret123 data\n");
    }

    #[test]
    fn test_redacting_writer_multiple_secrets_single_line() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        registry.add_secret("password456");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        writer
            .write_all(b"User secret123 has password456\n")
            .unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "User **** has ****\n");
    }

    #[test]
    fn test_redacting_writer_secrets_across_writes() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        // Write secret across multiple calls but within same line
        writer.write_all(b"This contains sec").unwrap();
        writer.write_all(b"ret123 data\n").unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "This contains **** data\n");
    }

    #[test]
    fn test_redacting_writer_multiple_lines() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        writer
            .write_all(b"Line 1 has secret123\nLine 2 is clean\nLine 3 has secret123 again\n")
            .unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(
            result,
            "Line 1 has ****\nLine 2 is clean\nLine 3 has **** again\n"
        );
    }

    #[test]
    fn test_redacting_writer_non_utf8_passthrough() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        // Write invalid UTF-8 bytes
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD, b'\n'];
        writer.write_all(&invalid_utf8).unwrap();
        writer.flush().unwrap();

        // Should pass through unchanged
        assert_eq!(output, invalid_utf8);
    }

    #[test]
    fn test_redacting_writer_write_line_convenience() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        writer.write_line("This contains secret123 data").unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "This contains **** data\n");
    }

    #[test]
    fn test_redacting_writer_flush_partial_line() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        // Write without newline and flush
        writer.write_all(b"This contains secret123 data").unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "This contains **** data");
    }

    #[test]
    fn test_redacting_writer_custom_placeholder() {
        let registry = SecretRegistry::new();
        registry.add_secret("secret123");
        let config = RedactionConfig::with_placeholder_and_registry(
            "[HIDDEN]".to_string(),
            registry.clone(),
        );

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        writer.write_line("This contains secret123 data").unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "This contains [HIDDEN] data\n");
    }

    #[test]
    fn test_hash_based_redaction() {
        let registry = SecretRegistry::new();
        let secret = "test-secret-value";
        registry.add_secret(secret);

        // Get the SHA-256 hash of the secret
        let hash = sha256_hash(secret);

        // Create text containing the hash
        let text_with_hash = format!("Log entry with hash: {}", hash);

        // The hash should be redacted
        let redacted = registry.redact_text(&text_with_hash);
        assert!(redacted.contains("****"));
        assert!(!redacted.contains(&hash));
    }

    #[test]
    fn test_hash_based_redaction_with_config() {
        let registry = SecretRegistry::new();
        let secret = "my-api-key-12345";
        registry.add_secret(secret);

        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Get the SHA-256 hash
        let hash = sha256_hash(secret);

        // Text containing both secret and hash
        let text = format!("Secret: {} Hash: {}", secret, hash);

        let redacted = redact_if_enabled(&text, &config);

        // Both secret and hash should be redacted
        assert_eq!(redacted.matches("****").count(), 2);
        assert!(!redacted.contains(secret));
        assert!(!redacted.contains(&hash));
    }

    #[test]
    fn test_hash_redaction_minimum_length() {
        let registry = SecretRegistry::new();

        // Add a short secret that won't be stored
        registry.add_secret("short");

        // The count should be 0 since it's too short
        assert_eq!(registry.secret_count(), 0);

        // Add a secret that meets minimum length
        let long_secret = "long-secret-12345";
        registry.add_secret(long_secret);

        // This should be stored
        assert_eq!(registry.secret_count(), 1);

        // The hash should also be stored and redacted
        let hash = sha256_hash(long_secret);
        let text_with_hash = format!("Found hash: {}", hash);

        let redacted = registry.redact_text(&text_with_hash);
        assert!(redacted.contains("****"));
        assert!(!redacted.contains(&hash));
    }

    #[test]
    fn test_multiple_secrets_with_hashes() {
        let registry = SecretRegistry::new();
        let secret1 = "password123";
        let secret2 = "api-key-xyz";
        let secret3 = "token-abc-def";

        registry.add_secret(secret1);
        registry.add_secret(secret2);
        registry.add_secret(secret3);

        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Create text with secrets and their hashes
        let hash1 = sha256_hash(secret1);
        let hash2 = sha256_hash(secret2);
        let hash3 = sha256_hash(secret3);

        let text = format!(
            "Secrets: {} {} {} Hashes: {} {} {}",
            secret1, secret2, secret3, hash1, hash2, hash3
        );

        let redacted = redact_if_enabled(&text, &config);

        // All secrets and hashes should be redacted (6 total)
        assert_eq!(redacted.matches("****").count(), 6);
        assert!(!redacted.contains(secret1));
        assert!(!redacted.contains(secret2));
        assert!(!redacted.contains(secret3));
        assert!(!redacted.contains(&hash1));
        assert!(!redacted.contains(&hash2));
        assert!(!redacted.contains(&hash3));
    }

    #[test]
    fn test_cryptographic_hash_determinism() {
        // Verify that SHA-256 produces deterministic results
        let input = "deterministic-test";

        let hash1 = sha256_hash(input);
        let hash2 = sha256_hash(input);
        let hash3 = sha256_hash(input);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);

        // Verify it's a valid hex string of expected length (64 chars for SHA-256)
        assert_eq!(hash1.len(), 64);
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_collision_resistance() {
        // Different inputs should produce different hashes
        let inputs = vec!["secret1", "secret2", "password", "api-key", "token-xyz"];

        let mut hashes = std::collections::HashSet::new();

        for input in &inputs {
            let hash = sha256_hash(input);
            assert!(
                hashes.insert(hash.clone()),
                "Hash collision detected for input: {}",
                input
            );
        }

        // All hashes should be unique
        assert_eq!(hashes.len(), inputs.len());
    }

    #[test]
    fn test_redacting_writer_with_hashes() {
        let registry = SecretRegistry::new();
        let secret = "writer-secret-123";
        registry.add_secret(secret);

        let config = RedactionConfig::with_custom_registry(registry.clone());

        let mut output = Vec::new();
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        // Write both secret and its hash
        let hash = sha256_hash(secret);
        let line = format!("Secret: {} Hash: {}\n", secret, hash);

        writer.write_all(line.as_bytes()).unwrap();
        writer.flush().unwrap();

        let result = String::from_utf8(output).unwrap();

        // Both should be redacted
        assert_eq!(result.matches("****").count(), 2);
        assert!(!result.contains(secret));
        assert!(!result.contains(&hash));
    }

    #[test]
    fn test_no_redact_preserves_hashes() {
        let registry = SecretRegistry::new();
        let secret = "test-no-redact";
        registry.add_secret(secret);

        // Redaction disabled
        let config = RedactionConfig::disabled();

        let hash = sha256_hash(secret);
        let text = format!("Secret: {} Hash: {}", secret, hash);

        let result = redact_if_enabled(&text, &config);

        // Nothing should be redacted
        assert!(!result.contains("****"));
        assert_eq!(result, text);
    }

    #[test]
    fn test_hash_redaction_in_json() {
        let registry = SecretRegistry::new();
        let secret = "json-secret-value";
        registry.add_secret(secret);

        let config = RedactionConfig::with_custom_registry(registry.clone());

        let hash = sha256_hash(secret);
        let json = format!(
            r#"{{"secret": "{}", "hash": "{}", "other": "data"}}"#,
            secret, hash
        );

        let redacted = redact_if_enabled(&json, &config);

        // Both secret and hash should be redacted in JSON
        assert!(redacted.contains("****"));
        assert!(!redacted.contains(secret));
        assert!(!redacted.contains(&hash));
        assert!(redacted.contains("other"));
        assert!(redacted.contains("data"));
    }

    #[test]
    fn test_hash_redaction_performance() {
        use std::time::Instant;

        let registry = SecretRegistry::new();

        // Add 50 secrets
        for i in 0..50 {
            registry.add_secret(&format!("test-secret-{:03}", i));
        }

        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Create test text with some hashes
        let hash1 = sha256_hash("test-secret-010");
        let hash2 = sha256_hash("test-secret-020");
        let test_text = format!("Hashes: {} and {} in logs", hash1, hash2);

        // Measure performance
        let start = Instant::now();
        for _ in 0..100 {
            let _result = redact_if_enabled(&test_text, &config);
        }
        let duration = start.elapsed();

        // Should be fast - less than 50ms for 100 operations with 50 secrets
        assert!(
            duration.as_millis() < 50,
            "Hash redaction took too long: {:?}",
            duration
        );
    }
}
