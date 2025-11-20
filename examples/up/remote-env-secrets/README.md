# Remote Environment and Secrets Example

## Overview

This example demonstrates using `--remote-env` flags and secrets files to inject environment variables and sensitive data into dev containers.

## Configuration Features

- **devcontainer.json remoteEnv**: Basic environment variables
- **--remote-env flags**: Additional environment variables at runtime
- **Secrets files**: Sensitive data loaded from files (redacted in logs)

## Usage

### Basic Remote Environment

Use environment variables from devcontainer.json only:

```bash
deacon up --workspace-folder .
```

### Add Runtime Environment Variables

Add environment variables via flags:

```bash
deacon up --workspace-folder . \
  --remote-env "API_ENDPOINT=https://api.example.com" \
  --remote-env "FEATURE_FLAG_BETA=true" \
  --remote-env "MAX_WORKERS=4"
```

### Load from Secrets File

Load sensitive data from a file:

```bash
deacon up --workspace-folder . \
  --secrets-file secrets.env
```

The secrets file format is:
```
KEY1=value1
KEY2=value2
```

### Combine Config, Flags, and Secrets

All sources are merged (secrets files are processed last):

```bash
deacon up --workspace-folder . \
  --remote-env "API_ENDPOINT=https://staging.example.com" \
  --remote-env "CACHE_TTL=3600" \
  --secrets-file secrets.env \
  --secrets-file env.env
```

Precedence order (lowest to highest):
1. devcontainer.json `remoteEnv`
2. `--remote-env` flags (in order)
3. Secrets files (in order)

## Environment Variable Format

The `--remote-env` flag requires `KEY=VALUE` format:

```bash
--remote-env "VAR_NAME=value"
--remote-env "PATH=/usr/local/bin:$PATH"
--remote-env "MULTI_LINE=line1\nline2"
```

## Secrets File Format

Secrets files follow the standard `.env` format:

```env
# Comments are supported
API_KEY=your_api_key_here
DB_PASSWORD=your_password

# Multi-line values with quotes
CERTIFICATE="-----BEGIN CERTIFICATE-----
...certificate content...
-----END CERTIFICATE-----"

# Empty values
OPTIONAL_VAR=
```

## Secrets Redaction

Secrets are automatically redacted in logs and output:
- Values from secrets files are never displayed in full
- Log output shows `<redacted>` instead of actual values
- JSON output does not include secret values

## Testing Environment Variables

Verify environment variables are set:

```bash
# Check all environment variables
docker exec <container-id> env

# Check specific variable
docker exec <container-id> printenv API_KEY

# Verify in lifecycle commands
docker exec <container-id> sh -c 'echo "DB_HOST: $DB_HOST"'
```

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

Note: Secrets are not included in the JSON output.

## Variable Substitution

Environment variables support substitution:

```json
{
  "remoteEnv": {
    "WORKSPACE_NAME": "${localWorkspaceFolderBasename}",
    "CONFIG_FILE": "${containerWorkspaceFolder}/config.json",
    "USER_HOME": "${containerEnv:HOME}"
  }
}
```

Available substitution variables:
- `${localWorkspaceFolder}`: Host workspace path
- `${localWorkspaceFolderBasename}`: Workspace directory name
- `${containerWorkspaceFolder}`: Container workspace path
- `${containerEnv:VAR}`: Container environment variable

## Security Best Practices

1. **Never commit secrets files to version control**
   ```gitignore
   secrets.env
   *.secret
   .env.local
   ```

2. **Use different secrets per environment**
   ```bash
   --secrets-file secrets.development.env   # For local dev
   --secrets-file secrets.staging.env       # For staging
   --secrets-file secrets.production.env    # For production
   ```

3. **Restrict file permissions**
   ```bash
   chmod 600 secrets.env
   ```

4. **Use secret management tools**
   ```bash
   # Example: Load from 1Password
   op inject -i secrets.template.env -o secrets.env
   deacon up --workspace-folder . --secrets-file secrets.env
   ```

## Common Use Cases

### Database Connection
```bash
--remote-env "DB_HOST=localhost" \
--remote-env "DB_PORT=5432" \
--secrets-file db-credentials.env  # Contains DB_USER, DB_PASSWORD
```

### API Keys and Tokens
```bash
--secrets-file api-keys.env  # Contains API_KEY, API_SECRET, JWT_SECRET
```

### Feature Flags
```bash
--remote-env "FEATURE_NEW_DASHBOARD=true" \
--remote-env "FEATURE_BETA_SEARCH=false"
```

### CI/CD Secrets
```bash
# In CI environment
echo "$CI_SECRETS" > /tmp/secrets.env
deacon up --workspace-folder . --secrets-file /tmp/secrets.env
```

## Cleanup

```bash
docker rm -f <container-id>

# Securely remove secrets files
shred -u secrets.env  # Linux
rm -P secrets.env     # macOS
```

## Related Examples

- `basic-image/` - Simple setup without environment variables
- `lifecycle-hooks/` - Using environment in lifecycle commands
- `compose-basic/` - Environment variables in Compose
