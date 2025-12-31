# OCI Registry Authentication

This guide explains how to authenticate with OCI registries for push/pull operations.

## Overview

Deacon supports multiple authentication methods for OCI registries:
- **Environment Variables**: Quick setup for CI/CD
- **Docker Config**: Leverages existing Docker credentials
- **Command-line Options**: Explicit credentials for specific operations

## Authentication Methods

### 1. Environment Variables

The simplest method for automated workflows and CI/CD pipelines.

#### Basic Authentication (Username/Password)

```bash
export DEACON_REGISTRY_USER="myusername"
export DEACON_REGISTRY_PASS="mypassword"

# Now push/pull operations will use these credentials
deacon features pull ghcr.io/myorg/my-feature:latest
```

#### Bearer Token Authentication

```bash
export DEACON_REGISTRY_TOKEN="ghp_abcdef123456..."

# All registry operations will use the bearer token
deacon features publish ./my-feature --registry ghcr.io/myorg/my-feature:1.0.0
```

### 2. Docker Config Integration

Deacon automatically reads credentials from `~/.docker/config.json`:

```bash
# Login with Docker CLI (credentials are stored)
docker login ghcr.io -u myusername -p mypassword

# Deacon will use the stored credentials automatically
deacon features pull ghcr.io/myorg/my-feature:latest
```

**Docker config location**: `~/.docker/config.json`

Example Docker config structure:
```json
{
  "auths": {
    "ghcr.io": {
      "auth": "dXNlcm5hbWU6cGFzc3dvcmQ="
    }
  }
}
```

### 3. Command-Line Options

Pass credentials directly for individual operations:

```bash
# Using username with password from stdin
echo "mypassword" | deacon features publish ./my-feature \
  --registry ghcr.io/myorg/my-feature:1.0.0 \
  --username myusername \
  --password-stdin
```

## Registry-Specific Setup

### GitHub Container Registry (ghcr.io)

#### Personal Access Token (PAT)

1. Generate a PAT: GitHub Settings → Developer settings → Personal access tokens → Tokens (classic)
2. Select scopes: `write:packages`, `read:packages`, `delete:packages`
3. Use the token as password:

```bash
export DEACON_REGISTRY_USER="your-github-username"
export DEACON_REGISTRY_PASS="ghp_your_pat_token"

# Or use Docker login
echo "ghp_your_pat_token" | docker login ghcr.io -u your-github-username --password-stdin
```

#### GitHub Actions

Use the built-in `GITHUB_TOKEN`:

```yaml
- name: Publish feature to GHCR
  env:
    DEACON_REGISTRY_USER: ${{ github.actor }}
    DEACON_REGISTRY_PASS: ${{ secrets.GITHUB_TOKEN }}
  run: |
    deacon features publish ./my-feature \
      --registry ghcr.io/${{ github.repository }}/my-feature:${{ github.ref_name }}
```

### Docker Hub

```bash
# Using Docker CLI
docker login -u your-dockerhub-username

# Or using environment variables
export DEACON_REGISTRY_USER="your-dockerhub-username"
export DEACON_REGISTRY_PASS="your-dockerhub-password"
```

### Private Registry (Self-Hosted)

```bash
export DEACON_REGISTRY_USER="admin"
export DEACON_REGISTRY_PASS="registry-password"

deacon features pull registry.example.com:5000/my-feature:latest
```

## Authentication Troubleshooting

### Common Issues

#### 1. Authentication Failed (401)

**Error**: `Authentication failed for URL: https://ghcr.io/v2/...`

**Solutions**:
- Verify credentials are correct
- For GitHub, ensure PAT has required scopes (`write:packages`, `read:packages`)
- Check if token has expired
- Ensure repository/package visibility allows access

#### 2. Connection Failed

**Error**: `Connection failed for URL: https://registry.example.com/v2/... Check if the registry is accessible`

**Solutions**:
- Verify network connectivity: `ping registry.example.com`
- Check if registry URL is correct
- Ensure firewall allows outbound HTTPS (port 443)
- Verify registry is running: `curl -I https://registry.example.com/v2/`

#### 3. Permission Denied (403)

**Error**: `HTTP 403 for URL: https://ghcr.io/v2/...`

**Solutions**:
- Verify user has push/pull permissions for the repository
- For GitHub, check repository package permissions
- Ensure authentication method provides correct permissions

### Testing Authentication

```bash
# Test pull access (read-only)
deacon features pull ghcr.io/myorg/test-feature:latest

# Test push access (write required)
deacon features publish ./test-feature \
  --registry ghcr.io/myorg/test-feature:test \
  --dry-run
```

## Security Best Practices

### 1. Never Commit Credentials

❌ **Don't do this**:
```bash
# .env file committed to Git
DEACON_REGISTRY_PASS="mypassword"
```

✅ **Do this instead**:
```bash
# Use environment-specific config (not in Git)
# Or use secret management tools
export DEACON_REGISTRY_PASS="$(vault read -field=password secret/registry)"
```

### 2. Use Least Privilege

- Grant minimal required permissions (read-only for pull, write for push)
- Use separate credentials for different environments
- Rotate tokens regularly

### 3. Secure CI/CD

```yaml
# GitHub Actions - Use encrypted secrets
env:
  DEACON_REGISTRY_USER: ${{ secrets.REGISTRY_USER }}
  DEACON_REGISTRY_PASS: ${{ secrets.REGISTRY_PASS }}
```

### 4. Token Expiration

- Set expiration dates on PATs and tokens
- Implement token rotation schedules
- Monitor for expired credentials in CI/CD

## Retry and Error Handling

Deacon implements automatic retry with exponential backoff for:
- Network timeouts
- Temporary connection failures  
- Rate limiting (429 responses)

**Default retry behavior**:
- Max attempts: 3
- Base delay: 1 second
- Max delay: 10 seconds
- Exponential backoff with jitter

Network errors are automatically retried, while authentication errors fail fast after initial retry.

## Offline Mode

When working offline or without network access:
- Pull operations will fail with clear error messages
- Push operations will fail with connection errors
- Cached features remain available for use
- Use `--dry-run` flag to validate without network access

## Related Documentation

- [Dry-Run Publish](../dry-run-publish/README.md) - Test publish operations without network
- [subcommand-specs/*/SPEC.md](../../../docs/subcommand-specs/*/SPEC.md) - OCI Registry Integration section
- [Feature Management](../../feature-management/README.md) - Feature lifecycle
