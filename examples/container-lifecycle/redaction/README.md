# Redaction Example

## What This Demonstrates

This example shows how secret redaction works in deacon CLI output:

- **Automatic redaction** of secret values in command output
- **Environment variable redaction** (API_KEY, DATABASE_PASSWORD)
- **Default redaction behavior** vs `--no-redact` flag
- **Progress file redaction** for structured events

## Redaction System Overview

According to the implementation in `crates/core/src/redaction.rs`, deacon automatically redacts sensitive values that appear in:
- Command strings (in progress events)
- Command output (stdout/stderr)
- Progress event JSON files
- Log messages

### What Gets Redacted

The redaction system tracks secrets from:
1. **Environment variables with sensitive names**: API_KEY, PASSWORD, SECRET, TOKEN, etc.
2. **Secrets loaded from files**: Via `--secrets-file` flag
3. **Values registered in the secret registry**: Automatically populated from containerEnv

### Redaction Placeholder

Redacted values are replaced with: `****`

## Testing Redaction

### 1. Configuration Validation

First, verify the configuration contains secrets:

```bash
deacon read-configuration --config devcontainer.json | jq '.containerEnv'
```

Expected output:
```json
{
  "PUBLIC_VALUE": "this-is-public-info",
  "API_KEY": "secret-api-key-12345",
  "DATABASE_PASSWORD": "super-secret-password"
}
```

### 2. Default Redaction (Enabled)

With default settings, secrets are automatically redacted:

```bash
# In a real scenario (requires Docker):
# deacon up --workspace-folder . --progress json --progress-file progress.json
#
# What happens:
# - Command output shows: "Testing secret: **** should be redacted"
# - Progress events show: "command": "echo 'postCreate: Testing secret: **** ...'"
# - Actual secret values are hidden in all output
```

#### Verify redaction in progress file
```bash
# After running with Docker:
# cat progress.json | jq 'select(.type == "lifecycle.command.begin") | .command'
#
# Expected: Commands contain **** instead of actual secrets
# "echo 'postCreate: Testing secret: **** should be redacted'"
# "echo 'postCreate: Another secret: **** should also be redacted'"
```

### 3. Disable Redaction (--no-redact Flag)

**⚠️ WARNING**: Only use `--no-redact` for debugging in secure environments!

```bash
# In a real scenario (requires Docker):
# deacon up --workspace-folder . --no-redact --progress json --progress-file progress-unredacted.json
#
# What happens:
# - Command output shows: "Testing secret: secret-api-key-12345 should be redacted"
# - Progress events show full command with actual secrets
# - ALL secret values are visible in output
```

#### Compare redacted vs unredacted
```bash
# After running both commands:
# diff progress.json progress-unredacted.json
#
# Shows the difference between redacted (****) and unredacted (actual values)
```

### 4. Redaction in Different Output Formats

#### Terminal output (default)
```bash
# deacon up --workspace-folder .
# Secrets are redacted in real-time terminal output
```

#### JSON progress file
```bash
# deacon up --workspace-folder . --progress json --progress-file progress.json
# Secrets are redacted in the JSON file
```

#### Structured JSON logs
```bash
# DEACON_LOG_FORMAT=json deacon up --workspace-folder .
# Secrets are redacted in JSON log messages
```

## Redaction Examples

### Example 1: Command with Secret

**Original command:**
```bash
echo 'postCreate: Testing secret: secret-api-key-12345 should be redacted'
```

**Redacted output:**
```bash
echo 'postCreate: Testing secret: **** should be redacted'
```

### Example 2: Progress Event

**Original event:**
```json
{
  "type": "lifecycle.command.begin",
  "command": "echo 'Database password: super-secret-password'",
  "command_id": "postCreate.1"
}
```

**Redacted event:**
```json
{
  "type": "lifecycle.command.begin",
  "command": "echo 'Database password: ****'",
  "command_id": "postCreate.1"
}
```

### Example 3: Multiple Secrets

**Original command:**
```bash
echo 'API_KEY=secret-api-key-12345 PASSWORD=super-secret-password'
```

**Redacted output:**
```bash
echo 'API_KEY=**** PASSWORD=****'
```

## Secret Detection Patterns

The redaction system automatically detects secrets based on environment variable names:

### Sensitive Patterns (Redacted)
- `*_KEY` - API_KEY, SECRET_KEY
- `*_SECRET` - CLIENT_SECRET, AWS_SECRET
- `*_TOKEN` - AUTH_TOKEN, ACCESS_TOKEN
- `*_PASSWORD` - DB_PASSWORD, DATABASE_PASSWORD
- `*_PASS` - MYSQL_PASS, POSTGRES_PASS
- `*_CREDENTIAL` - API_CREDENTIAL

### Public Patterns (Not Redacted)
- `PUBLIC_*` - PUBLIC_VALUE, PUBLIC_KEY
- Regular values without sensitive keywords

## Advanced Usage

### Using Secrets File

For production, use `--secrets-file` to load secrets:

```bash
# Create secrets.env file
cat > secrets.env << 'EOF'
API_KEY=prod-api-key-xyz
DATABASE_PASSWORD=prod-db-pass-123
EOF

# Load secrets and run
# deacon up --secrets-file secrets.env --workspace-folder .
```

Secrets from the file are automatically registered for redaction.

### Custom Redaction Placeholder

The default placeholder is `****`, but you can customize it in code:

```rust
let config = RedactionConfig::with_placeholder("[REDACTED]".to_string());
```

### Verify Redaction Works

```bash
# After running with Docker and progress file:
# Ensure no secrets appear in output
cat progress.json | grep -i "secret-api-key-12345"
# Should return nothing (0 results)

cat progress.json | grep -i "super-secret-password"
# Should return nothing (0 results)

# Check that redaction marker exists
cat progress.json | grep "\\*\\*\\*\\*"
# Should return multiple matches
```

## jq Snippets for Verification

### Count redacted commands
```bash
cat progress.json | jq '[.command // ""] | map(select(contains("****"))) | length'
```

### Show all commands with potential secrets (before redaction)
```bash
cat progress-unredacted.json | jq 'select(.type == "lifecycle.command.begin") | select(.command | contains("secret") or contains("password")) | .command'
```

### Compare redaction effectiveness
```bash
# Extract commands from both files
cat progress.json | jq -r 'select(.type == "lifecycle.command.begin") | .command' > redacted.txt
cat progress-unredacted.json | jq -r 'select(.type == "lifecycle.command.begin") | .command' > unredacted.txt

# Compare
diff -u unredacted.txt redacted.txt
```

## Security Best Practices

1. **Always use default redaction** - Only disable for isolated debugging
2. **Never commit secrets to version control** - Use secrets files, environment variables, or vaults
3. **Verify redaction works** - Check progress files and logs for leaks
4. **Audit trail** - Progress files can be safely shared when redaction is enabled
5. **CI/CD integration** - Redaction protects secrets in build logs

## When to Use --no-redact

**⚠️ Use with extreme caution!**

Appropriate scenarios:
- Local debugging in a secure, isolated environment
- Troubleshooting redaction issues
- Comparing expected vs actual command execution
- Development on a personal machine with non-production secrets

**Never use in:**
- CI/CD pipelines
- Shared environments
- Production systems
- When output is logged or shared

## Implementation Details

### Secret Registry
The global secret registry (`crates/core/src/redaction.rs`) tracks all sensitive values:
- Populated from containerEnv at container creation
- Loaded from secrets files
- Scans for environment variables with sensitive names

### Redaction Points
Secrets are redacted at multiple points:
1. **Before command execution** - Command strings in progress events
2. **During output capture** - Stdout/stderr from command execution
3. **Progress file writing** - JSON events written to file
4. **Log emission** - Tracing logs and structured output

### Performance
Redaction is optimized:
- Pattern matching on secret registry entries
- No performance impact when no secrets are registered
- Efficient string replacement algorithms

## Key Takeaways

- **Redaction is enabled by default** - Protects secrets automatically
- **Works across all output** - Terminal, files, logs, events
- **Pattern-based detection** - Recognizes common secret patterns
- **--no-redact flag available** - For controlled debugging only
- **Verify with jq** - Easy to confirm redaction effectiveness
- **Safe for CI/CD** - Progress files can be archived with confidence

## References

- Implementation: `crates/core/src/redaction.rs`
- Tests: `crates/core/src/progress.rs` (test_redacting_*)
- Secret loading: `crates/core/src/secrets.rs`
- Related issues: #110, #115, #125
- Up SPEC: Security and redaction: ../../../docs/subcommand-specs/up/SPEC.md#12-security-considerations
