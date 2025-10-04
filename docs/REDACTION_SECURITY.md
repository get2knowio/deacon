# Redaction Security Architecture

## Overview

This document describes the cryptographic redaction system implemented in deacon, which provides security hardening for secret values across all output streams.

## Architecture

### Core Components

#### 1. SecretRegistry (`crates/core/src/redaction.rs`)

Thread-safe registry that stores secrets in multiple formats:
- **Exact secrets**: Original secret strings for exact matching
- **SHA-256 hashes**: Cryptographic hashes for additional detection
- **Structured secrets**: Context-aware secrets with key-value pair matching

```rust
pub struct SecretRegistry {
    inner: Arc<RwLock<SecretRegistryInner>>,
}

struct SecretRegistryInner {
    exact_secrets: HashSet<String>,
    secret_hashes: HashSet<String>,
    structured_secrets: Vec<StructuredSecret>,
}
```

#### 2. RedactingWriter (`crates/core/src/redaction.rs`)

Stream-based redaction wrapper that applies redaction at line boundaries:
- Buffers partial lines until newline
- Applies redaction to complete lines
- Forwards redacted output to underlying writer

```rust
pub struct RedactingWriter<W> {
    inner: W,
    buffer: Vec<u8>,
    config: RedactionConfig,
    registry: SecretRegistry,
}
```

#### 3. RedactionConfig

Configuration object that controls redaction behavior:
- `enabled`: Whether redaction is active
- `placeholder`: Custom replacement text (default: `****`)
- `custom_registry`: Optional registry for testing

### Cryptographic Hashing

The system uses **SHA-256** for secure secret hashing:

```rust
fn sha256_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

Properties:
- **Deterministic**: Same input always produces same hash
- **One-way**: Cannot reverse hash to recover secret
- **Collision-resistant**: Different inputs produce different hashes
- **Fixed-length**: Always 64 hex characters (256 bits)

### Redaction Flow

```
Secret Registration
    ↓
Store Exact + SHA-256 Hash
    ↓
Output Generation
    ↓
Apply Redaction (exact & hash matching)
    ↓
Redacted Output
```

## Output Sinks

All output sinks apply consistent redaction:

### 1. Progress Events / Audit Logs

**Location**: `crates/core/src/progress.rs`

```rust
pub fn log_event(&mut self, event: &ProgressEvent) -> Result<()> {
    let line = serde_json::to_string(event)?;
    let redacted_line = redact_if_enabled(&line, &self.redaction_config);
    writeln!(self.file, "{}", redacted_line)?;
    // ...
}
```

### 2. Doctor Output

**Location**: `crates/core/src/doctor.rs`

Text output:
```rust
macro_rules! println_redacted {
    ($config:expr, $fmt:expr, $($arg:tt)*) => {
        let output = format!($fmt, $($arg)*);
        let redacted = redact_if_enabled(&output, $config);
        println!("{}", redacted);
    };
}
```

JSON output:
```rust
let json_output = serde_json::to_string_pretty(&doctor_info)?;
let redacted_output = redact_if_enabled(&json_output, &redaction_config);
println!("{}", redacted_output);
```

### 3. PORT_EVENT Output

**Location**: `crates/core/src/ports.rs`

```rust
fn emit_port_event(event: &PortEvent, ...) {
    let output_line = format!("PORT_EVENT: {}", json);
    let mut stdout = std::io::stdout();
    let mut redacting_writer = RedactingWriter::new(&mut stdout, config, registry);
    redacting_writer.write_line(&output_line)?;
}
```

### 4. Lifecycle Command Output

**Location**: `crates/core/src/lifecycle.rs`

```rust
let redacted_line = redact_if_enabled(&line, &ctx.redaction_config);
output_sink.write_line(&redacted_line)?;
```

## --no-redact Flag

The `--no-redact` CLI flag provides emergency override:

**Location**: `crates/deacon/src/cli.rs`

```rust
let redaction_config = if self.no_redact {
    RedactionConfig::disabled()
} else {
    RedactionConfig::default()
};
```

**⚠️ WARNING**: Only use `--no-redact` in secure, isolated environments for debugging.

## Security Properties

### 1. Defense in Depth

Multiple layers of secret detection:
- Exact string matching
- SHA-256 hash matching
- Context-aware structured secrets

### 2. Minimum Length Threshold

Secrets shorter than 8 characters are not stored (prevents false positives):

```rust
const MIN_REDACTION_LENGTH: usize = 8;
```

### 3. Thread Safety

All operations use `Arc<RwLock<...>>` for safe concurrent access:
- Multiple threads can read simultaneously
- Writers block other writers
- No data races or corruption

### 4. Performance

- Hash lookup: O(n) where n = number of registered secrets
- String replacement: O(m) where m = output length
- Acceptable for typical secret counts (<100)
- Performance tests verify <50ms for 100 operations with 50 secrets

## Test Coverage

### Unit Tests (43 tests in `crates/core/src/redaction.rs`)

Key tests:
- `test_hash_based_redaction` - Hash redaction works
- `test_cryptographic_hash_determinism` - SHA-256 is deterministic
- `test_hash_collision_resistance` - Different inputs yield different hashes
- `test_multiple_secrets_with_hashes` - Multiple secrets/hashes redacted
- `test_no_redact_preserves_hashes` - Flag disables redaction
- `test_hash_redaction_performance` - Performance acceptable

### Integration Tests (35 tests in `crates/core/tests/integration_redaction.rs`)

Key tests:
- `test_hash_redaction_in_lifecycle` - Lifecycle commands redact hashes
- `test_redaction_consistency_across_outputs` - All sinks consistent
- `test_doctor_output_redaction` - Doctor output redacted
- `test_audit_log_redaction` - Audit logs redacted
- `test_redacting_writer_port_events` - PORT_EVENT redacted
- `test_cryptographic_hash_security_properties` - Hash security validated

## Usage Examples

### Basic Registration and Redaction

```rust
use deacon_core::redaction::{add_global_secret, redact_if_enabled, RedactionConfig};

// Register a secret
add_global_secret("my-api-key-12345");

// Redact output
let config = RedactionConfig::default();
let text = "API key is my-api-key-12345";
let redacted = redact_if_enabled(text, &config);
// Result: "API key is ****"
```

### Hash-based Redaction

```rust
// Secret is automatically hashed on registration
add_global_secret("secret-value");

// Both secret and hash are redacted
let hash = sha256_hash("secret-value");
let text = format!("Secret: secret-value Hash: {}", hash);
let redacted = redact_if_enabled(&text, &config);
// Result: "Secret: **** Hash: ****"
```

### Stream-based Redaction

```rust
use deacon_core::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};

let registry = SecretRegistry::new();
registry.add_secret("stream-secret");
let config = RedactionConfig::with_custom_registry(registry.clone());

let mut output = Vec::new();
let mut writer = RedactingWriter::new(&mut output, config, &registry);

writer.write_line("This contains stream-secret")?;
// Output: "This contains ****\n"
```

### Structured Secrets

```rust
use deacon_core::redaction::StructuredSecret;

let registry = SecretRegistry::new();

// Only redact in key-value context
let structured = StructuredSecret::new(
    "common-value".to_string(),
    Some("password".to_string()),
    None,
    true,
).unwrap();
registry.add_structured_secret(structured);

// Not redacted in normal text
let text1 = "common-value appears here";
// Result: "common-value appears here"

// Redacted in key-value context
let text2 = "password=common-value";
// Result: "password=****"
```

## CLI-SPEC.md Alignment

This implementation satisfies the requirements in `docs/CLI-SPEC.md`:

### Security and Permissions Section
- ✅ Secret Management: Secure handling of secrets and credentials
- ✅ Redaction system applies consistently across both stdout and stderr

### Output & Logging Stream Contract
- ✅ Structured progress events (redacted)
- ✅ Audit trail (redacted)
- ✅ PORT_EVENT lines (redacted)
- ✅ JSON output format (redacted)

## References

- Implementation: `crates/core/src/redaction.rs`
- Progress integration: `crates/core/src/progress.rs`
- Doctor integration: `crates/core/src/doctor.rs`
- Ports integration: `crates/core/src/ports.rs`
- Lifecycle integration: `crates/core/src/lifecycle.rs`
- Unit tests: `crates/core/src/redaction.rs` (tests module)
- Integration tests: `crates/core/tests/integration_redaction.rs`
- CLI flag handling: `crates/deacon/src/cli.rs`
- Related issues: #110, #115, #125 (referenced in examples)
