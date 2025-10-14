# Secrets and SSH Build Example

## What This Demonstrates

This example shows BuildKit-dependent features:

- **Secret Mounts**: Securely passing sensitive data during build without storing in layers
- **SSH Forwarding**: Using SSH agent for private repository access during build
- **BuildKit Requirement**: Understanding when BuildKit is required vs optional

## Why This Matters

BuildKit advanced features are essential for:
- **Security**: Secrets never appear in image layers or build history
- **Private Dependencies**: Accessing private Git repos, npm registries, etc.
- **CI/CD Security**: Safely using credentials in automated builds
- **Build Isolation**: Ensuring secrets are only available during specific build steps

## DevContainer Specification References

This example aligns with:
- **[Build Options](https://containers.dev/implementors/spec/#build-properties)**: Advanced BuildKit features
- CLI Spec: Container Build section in `docs/subcommand-specs/*/SPEC.md`

## Files

- `Dockerfile`: Demonstrates `--mount=type=secret` and `--mount=type=ssh` syntax
- `devcontainer.json`: Basic DevContainer configuration

## BuildKit Requirement

The features in this example **require BuildKit**. The `deacon` CLI will:
- Use BuildKit automatically when `--secret` or `--ssh` flags are present
- Error if you explicitly disable BuildKit with `--buildkit never` while using secrets/SSH

## Run

### Build with Secret Mount

Create a test secret file:
```sh
echo "my-secret-value" > /tmp/test-secret.txt
```

Build with the secret:
```sh
deacon build --workspace-folder . --secret id=foo,src=/tmp/test-secret.txt
```

Or use an empty secret for testing (demonstrates syntax):
```sh
deacon build --workspace-folder . --secret id=foo,src=/dev/null
```

### Build with SSH Forwarding

Use your SSH agent:
```sh
deacon build --workspace-folder . --ssh default
```

Or specify a specific SSH agent socket:
```sh
deacon build --workspace-folder . --ssh default=$SSH_AUTH_SOCK
```

### Combine Secrets and SSH
```sh
deacon build --workspace-folder . \
  --secret id=foo,src=/tmp/test-secret.txt \
  --ssh default
```

### Force BuildKit
While BuildKit is automatic with secrets/SSH, you can also explicitly enable it:
```sh
deacon build --workspace-folder . --buildkit auto --secret id=foo,src=/dev/null
```

## Validation

### Check BuildKit Was Used

When BuildKit is active, you'll see output like:
```
[+] Building 2.3s (8/8) FINISHED
 => [internal] load build definition from Dockerfile
 => => transferring dockerfile: 234B
 => [internal] load .dockerignore
 => [internal] load metadata for docker.io/library/alpine:3.19
```

Without BuildKit, the output format is different:
```
Sending build context to Docker daemon  2.048kB
Step 1/3 : FROM alpine:3.19
```

### Verify Secret Handling

Check that secrets don't appear in the image:

```sh
IMAGE_ID="<image-id-from-build-output>"

# Search for the secret value in the image layers
docker history "$IMAGE_ID" --no-trunc | grep "my-secret-value" || echo "Secret not found in history (good!)"

# Verify the build completed successfully
docker run --rm "$IMAGE_ID" cat /buildkit-test.txt
```

You should see:
- Secret NOT found in image history ✓
- Build completed marker file exists ✓

### Check SSH Mount Behavior

If you have an SSH agent running:

```sh
# Start SSH agent if needed
eval $(ssh-agent -s)
ssh-add ~/.ssh/id_rsa  # or your key path

# Build with SSH forwarding
deacon build --workspace-folder . --ssh default --output-format json
```

The build should complete successfully and show SSH agent availability in build logs.

## Error Scenarios

### Missing BuildKit

If BuildKit is not available or disabled:

```sh
# This will fail with a clear error
deacon build --workspace-folder . --buildkit never --secret id=foo,src=/dev/null
```

Expected error:
```
Error: The --secret/--ssh options require BuildKit but --buildkit never was specified
```

### Without Explicit Flags

If you try to build without using the `--secret` or `--ssh` CLI flags, the build will fail because the Dockerfile has `--mount` directives:

```sh
# This will fail
deacon build --workspace-folder .
```

Expected error from Docker:
```
failed to solve: failed to process ...: the --mount option requires BuildKit.
```

Solution: Use `--secret` or `--ssh` flags, or set `DOCKER_BUILDKIT=1` environment variable.

## Secret Types

BuildKit supports several secret sources:

### File-based Secrets
```sh
deacon build --workspace-folder . --secret id=foo,src=/path/to/secret.txt
```

### Environment Variable Secrets
```sh
export MY_SECRET="sensitive-value"
deacon build --workspace-folder . --secret id=foo,env=MY_SECRET
```

### Empty Secrets (for testing)
```sh
deacon build --workspace-folder . --secret id=foo,src=/dev/null
```

## SSH Mount Types

### Default Agent
Uses the default SSH agent:
```sh
deacon build --workspace-folder . --ssh default
```

### Specific Socket
```sh
deacon build --workspace-folder . --ssh default=/path/to/ssh/socket
```

### Multiple Keys
```sh
deacon build --workspace-folder . --ssh github=$SSH_AUTH_SOCK --ssh gitlab=/run/ssh-agent2.sock
```

## Security Best Practices

### DO:
- ✓ Use secret mounts for API keys, tokens, passwords
- ✓ Use SSH forwarding for private Git repositories
- ✓ Keep secrets in `--mount` directives, never in `RUN` commands
- ✓ Clean up test secrets after building

### DON'T:
- ✗ Store secrets in environment variables (they persist in the image)
- ✗ Use secrets in `RUN` commands without `--mount`
- ✗ Commit secret files to version control
- ✗ Use `ADD` or `COPY` for secrets

## Expected Output

### Successful Build with Secrets
```
[+] Building 3.2s (10/10) FINISHED
 => [internal] load build definition from Dockerfile
 => [internal] load .dockerignore
 => [internal] load metadata for docker.io/library/alpine:3.19
 => [1/4] FROM docker.io/library/alpine:3.19
 => [2/4] RUN apk add --no-cache git openssh-client
 => [3/4] RUN --mount=type=secret,id=foo ...
 => [4/4] RUN echo "Build completed with BuildKit features"
 => exporting to image
Successfully built image: sha256:abc123...
```

### With JSON Output
```json
{
  "image_id": "sha256:abc123...",
  "config_hash": "def456...",
  "build_duration": "4.1s"
}
```

## Cleanup

Remove test secrets:
```sh
rm -f /tmp/test-secret.txt
```

Remove built images:
```sh
docker images --filter "label=example.type=secrets-and-ssh" -q | xargs -r docker rmi
```

## Real-World Use Cases

### Private NPM Packages
```dockerfile
RUN --mount=type=secret,id=npmrc,target=/root/.npmrc \
    npm install
```

### Private Git Repositories
```dockerfile
RUN --mount=type=ssh \
    git clone git@github.com:private/repo.git
```

### Database Migrations
```dockerfile
RUN --mount=type=secret,id=db_password \
    DB_PASSWORD=$(cat /run/secrets/db_password) \
    npm run migrate
```

### Docker Registry Authentication
```dockerfile
RUN --mount=type=secret,id=docker_auth,target=/root/.docker/config.json \
    docker pull private-registry.com/image:latest
```

## See Also

- `../basic-dockerfile/` - Basic Dockerfile builds with build args
- `../platform-and-cache/` - Platform targeting and cache control
- [BuildKit documentation](https://docs.docker.com/build/buildkit/)
- [Docker build secrets](https://docs.docker.com/build/building/secrets/)
- CLI Spec: Container Build section in `docs/subcommand-specs/*/SPEC.md`
