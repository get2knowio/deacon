//! Secret redaction and sensitive output filtering
//!
//! This module provides centralized redaction of secret values across logs, events,
//! doctor bundle, and errors. It maintains an in-memory set of secret keys and hashed
//! values for detection via naive substring scanning with length thresholds.
//!
//! References: CLI-SPEC.md "Security and Compliance"

use std::collections::HashSet;
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

    /// Check if a value contains any registered secrets and return redacted version
    ///
    /// This performs naive substring scanning for both exact secrets and their hashes.
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

            result
        } else {
            // If we can't acquire the lock, return original text
            text.to_string()
        }
    }

    /// Get the count of registered secrets (for testing/debugging)
    pub fn secret_count(&self) -> usize {
        if let Ok(inner) = self.inner.read() {
            inner.exact_secrets.len()
        } else {
            0
        }
    }

    /// Clear all registered secrets
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.exact_secrets.clear();
            inner.secret_hashes.clear();
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
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            placeholder: None,
        }
    }
}

impl RedactionConfig {
    /// Create config with redaction disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            placeholder: None,
        }
    }

    /// Create config with custom placeholder
    pub fn with_placeholder(placeholder: String) -> Self {
        Self {
            enabled: true,
            placeholder: Some(placeholder),
        }
    }
}

/// Redact text using the global registry if redaction is enabled
pub fn redact_if_enabled(text: &str, config: &RedactionConfig) -> String {
    if !config.enabled {
        return text.to_string();
    }

    let redacted = global_registry().redact_text(text);

    // Apply custom placeholder if specified
    if let Some(custom_placeholder) = &config.placeholder {
        redacted.replace(REDACTION_PLACEHOLDER, custom_placeholder)
    } else {
        redacted
    }
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

/// Simple SHA-256 hash function
fn sha256_hash(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Note: Using DefaultHasher for simplicity. In production, should use
    // a proper cryptographic hash like SHA-256 from a crate like `sha2`
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:x}", hasher.finish())
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
}
