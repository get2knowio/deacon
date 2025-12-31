# Build Secrets Example

This example demonstrates how to use the `--build-secret` flag to securely pass secrets to Docker BuildKit builds.

## Overview

The `--build-secret` flag allows you to mount secrets into your Docker build process without persisting them in the final image layers. This is useful for:

- Fetching private dependencies during build
- Authenticating with private registries
- Using API tokens in build scripts
- Any scenario where you need credentials during build but not in the final image

## Syntax

```bash
deacon build --build-secret id=<id>[,src=<path>|env=<var>|stdin|value-stdin]
```

Three source types are supported:

1. **File source**: `--build-secret id=mytoken,src=/path/to/secret.txt`
2. **Environment variable**: `--build-secret id=mytoken,env=MY_SECRET_VAR`
3. **Stdin**: 
   - Implicit (default): `--build-secret id=mytoken`
   - Explicit: `--build-secret id=mytoken,stdin` or `--build-secret id=mytoken,value-stdin`

## Example: Using a GitHub Token

### 1. Create a Dockerfile with Secret Mount

```dockerfile
# syntax=docker/dockerfile:1
FROM node:18-alpine

# Mount the GitHub token as a secret during npm install
RUN --mount=type=secret,id=github_token \
    npm config set //npm.pkg.github.com/:_authToken=$(cat /run/secrets/github_token) && \
    npm install @myorg/private-package && \
    npm config delete //npm.pkg.github.com/:_authToken

# The secret is NOT in the final image
COPY . .
CMD ["npm", "start"]
```

### 2. Build with Secret from File

```bash
echo "ghp_yourGitHubTokenHere" > github_token.txt
deacon build --build-secret id=github_token,src=github_token.txt --buildkit auto
rm github_token.txt  # Clean up
```

### 3. Build with Secret from Environment

```bash
export GITHUB_TOKEN="ghp_yourGitHubTokenHere"
deacon build --build-secret id=github_token,env=GITHUB_TOKEN --buildkit auto
```

### 4. Build with Secret from Stdin

```bash
# Using implicit stdin (no source specified)
echo "ghp_yourGitHubTokenHere" | deacon build --build-secret id=github_token --buildkit auto

# Or explicitly specify stdin
echo "ghp_yourGitHubTokenHere" | deacon build --build-secret id=github_token,stdin --buildkit auto

# Or using the value-stdin flag
echo "ghp_yourGitHubTokenHere" | deacon build --build-secret id=github_token,value-stdin --buildkit auto
```
```

## Security Features

### Automatic Redaction

Secret values are automatically redacted from all log output:

```bash
$ deacon build --build-secret id=token,src=token.txt
# Output will show:
# "Registering build secret 'token' for redaction (length: 25)"
# But the actual token value "my-secret-value" will NEVER appear
```

### BuildKit Requirement

Build secrets require Docker BuildKit. If you try to use them without BuildKit:

```bash
$ deacon build --build-secret id=token,src=token.txt --buildkit never
Error: The --build-secret options require BuildKit
```

## Validation

The CLI performs strict validation:

### Duplicate IDs

```bash
$ deacon build --build-secret id=token,src=file1 --build-secret id=token,src=file2
Error: Duplicate build secret id 'token'. Each secret must have a unique id.
```

### Missing Files

```bash
$ deacon build --build-secret id=token,src=/nonexistent/file
Error: Build secret file '/nonexistent/file' does not exist
```

### Missing Environment Variables

```bash
$ deacon build --build-secret id=token,env=UNDEFINED_VAR
Error: Build secret environment variable 'UNDEFINED_VAR' is not set
```

## devcontainer.json Example

```json
{
  "name": "Node with Private Packages",
  "dockerFile": "Dockerfile",
  "build": {
    "context": ".",
    "options": {
      "BUILDKIT_INLINE_CACHE": "1"
    }
  }
}
```

Then build with:

```bash
deacon build --build-secret id=github_token,env=GITHUB_TOKEN --buildkit auto
```

## Best Practices

1. **Never commit secrets to version control** - use `.gitignore` for secret files
2. **Use environment variables** for CI/CD pipelines
3. **Prefer file sources** for local development
4. **Always use BuildKit** - it's more secure and efficient
5. **Clean up temporary secret files** after the build
6. **Use unique IDs** for different secrets in the same build

## Dockerfile Secret Mount Syntax

In your Dockerfile, access secrets using:

```dockerfile
RUN --mount=type=secret,id=<secret-id> \
    command-that-uses /run/secrets/<secret-id>
```

The secret is available at `/run/secrets/<secret-id>` during the RUN instruction only.

## See Also

- [Docker BuildKit Secrets Documentation](https://docs.docker.com/build/building/secrets/)
- [DevContainer Specification](https://containers.dev/)
- subcommand-specs/*/SPEC.md: Build Process Workflow
- subcommand-specs/*/SPEC.md: Security and Permissions
