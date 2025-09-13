# Deacon CLI Examples

This guide shows copy‑paste ready terminal commands (zsh/bash) to exercise the Deacon CLI end‑to‑end. Each section can be run independently. Commands avoid Rust/cargo internals and use the `deacon` binary directly.

Notes
- Run from the repo root unless `cd` is shown: `/workspaces/deacon`.
- Docker must be installed and running for container operations (build/up/exec/down/compose/features test).
- The CLI accepts JSONC configs too (our fixtures use `*.jsonc`).

## Quick Help
```sh
# Show global help and all commands
deacon --help

# Show help for a subcommand
deacon up --help
```

## Read Configuration
Parse and print the effective devcontainer configuration.
```sh
# Basic fixture (JSONC with comments)
deacon read-configuration \
  --workspace-folder . \
  --config fixtures/config/basic/devcontainer.jsonc

# With variable substitution and env passthrough
deacon read-configuration \
  --workspace-folder . \
  --config fixtures/config/with-variables/devcontainer.jsonc

# Include merged configuration with overrides and secrets file
# (creates a tiny override and secrets file in a temp directory)
TMPDIR=$(mktemp -d)
cat > "$TMPDIR/override.json" <<'JSON'
{ "containerEnv": { "OVERRIDE": "true" } }
JSON
cat > "$TMPDIR/secrets.env" <<'ENV'
API_KEY=example-secret
ENV
# Use repo root devcontainer if present; otherwise reuse a fixture
CONFIG=fixtures/config/basic/devcontainer.jsonc

deacon read-configuration \
  --workspace-folder . \
  --config "$CONFIG" \
  --override-config "$TMPDIR/override.json" \
  --secrets-file "$TMPDIR/secrets.env"
```

## Build Image
Build an image when `dockerFile` is used in the config. Demonstrates both JSON and text outputs.
```sh
# Create a scratch workspace with a Dockerfile and devcontainer.json
WORK=$(mktemp -d)
cat > "$WORK/Dockerfile" <<'DOCKER'
FROM alpine:3.19
RUN echo hi
DOCKER
mkdir -p "$WORK/.devcontainer"
cat > "$WORK/.devcontainer/devcontainer.json" <<'JSON'
{ "name": "BuildExample", "dockerFile": "Dockerfile", "build": {"context": "."} }
JSON

# JSON output
deacon --workspace-folder "$WORK" build --output-format json
# Text output
deacon --workspace-folder "$WORK" build

# With build args and platform
deacon --workspace-folder "$WORK" build \
  --output-format json \
  --build-arg BUILD_ENV=production \
  --platform linux/amd64
```

If Docker is unavailable, see the dedicated example near the end of this document.

## Up (Traditional), Exec, Down
Start a long‑running container, execute a command inside it, then stop it.
```sh
WS=$(mktemp -d)
mkdir -p "$WS/.devcontainer"
cat > "$WS/.devcontainer/devcontainer.json" <<'JSON'
{ "name": "UpExecExample", "image": "nginx:alpine", "workspaceFolder": "/workspace" }
JSON

# Bring the dev container up (recreate if exists, skip post-create for speed)
deacon --workspace-folder "$WS" up \
  --remove-existing-container \
  --skip-post-create \
  --skip-non-blocking-commands

# Run a command in the running container (TTY disabled for CI-friendliness)
deacon --workspace-folder "$WS" exec \
  --no-tty -- sh -lc 'echo -n OK: && whoami && pwd'

# Stop the container; add --remove to also remove it
deacon --workspace-folder "$WS" down
```

## Up (Compose)
Demonstrate automatic compose path when `dockerComposeFile` is present.
```sh
CS=$(mktemp -d)
cat > "$CS/docker-compose.yml" <<'YML'
version: '3.8'
services:
  app:
    image: alpine:3.19
    command: sleep infinity
    volumes:
      - .:/workspace:cached
    working_dir: /workspace
YML
mkdir -p "$CS/.devcontainer"
cat > "$CS/.devcontainer/devcontainer.json" <<'JSON'
{
  "name": "ComposeExample",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace"
}
JSON

# Start the compose project
deacon --workspace-folder "$CS" up
# Stop the compose project
deacon --workspace-folder "$CS" down
```

## Exec Working Directory and Env
`exec` sets the working directory to `workspaceFolder` and passes `--env`.
```sh
EW=$(mktemp -d)
mkdir -p "$EW/.devcontainer"
cat > "$EW/.devcontainer/devcontainer.json" <<'JSON'
{ "name": "ExecWD", "image": "alpine:3.19", "workspaceFolder": "/custom/workspace" }
JSON

deacon --workspace-folder "$EW" up --skip-post-create

deacon --workspace-folder "$EW" exec \
  --env FOO=bar -- sh -lc 'pwd && echo $FOO'

# Cleanup
deacon --workspace-folder "$EW" down
```

## Host Requirements (validation toggle)
If a config declares `hostRequirements`, `--ignore-host-requirements` allows continuing with warnings.
```sh
HR=$(mktemp -d)
mkdir -p "$HR/.devcontainer"
cat > "$HR/.devcontainer/devcontainer.json" <<'JSON'
{
  "name": "HostReqs",
  "image": "alpine:3.19",
  "hostRequirements": { "cpus": 1 }
}
JSON

# Validate strictly (default)
deacon --workspace-folder "$HR" up
# Ignore and continue
deacon --workspace-folder "$HR" up --ignore-host-requirements
```

## Feature Management
Test, package, and (dry‑run) publish features using fixtures.
```sh
# Test a minimal feature (requires Docker to run install.sh)
deacon features test fixtures/features/minimal --json

# Package a feature (creates a tarball and manifest in output dir)
OUT=$(mktemp -d)
deacon features package fixtures/features/with-options --output "$OUT" --json
ls -lah "$OUT"

# Dry‑run publish (real publishing not yet implemented)
deacon features publish fixtures/features/with-options --registry ghcr.io/example --dry-run --json
```

## Template Management
Show metadata, generate docs, and dry‑run publish templates.
```sh
# Metadata summary
deacon templates metadata fixtures/templates/minimal

# Generate README fragment
DOCS=$(mktemp -d)
deacon templates generate-docs fixtures/templates/with-options --output "$DOCS"
cat "$DOCS/README-template.md"

# Dry‑run publish (simulates an OCI package push)
deacon templates publish fixtures/templates/with-options --registry ghcr.io/example --dry-run
```

## Progress and Logging Controls
Adjust logging and emit structured progress events to a file.
```sh
PROGRESS=progress.json

# JSON logs and JSON progress events written to a file
deacon \
  --log-format json \
  --log-level debug \
  --progress json \
  --progress-file "$PROGRESS" \
  read-configuration --workspace-folder . --config fixtures/config/basic/devcontainer.jsonc

# Inspect the progress events
jq . "$PROGRESS" 2>/dev/null || cat "$PROGRESS"
```

## Doctor
Environment diagnostics and optional support bundle path.
```sh
# JSON diagnostics (logs may mix with JSON; extract with jq if needed)
deacon doctor --json

# Create a support bundle file
BUNDLE="doctor-$(date +%s).tar.gz"
deacon doctor --bundle "$BUNDLE" --json
ls -lah "$BUNDLE"
```

## Error Handling Examples (expected failures)
Useful for verifying helpful messages in edge cases.
```sh
# No config in current directory
tmp=$(mktemp -d)
( cd "$tmp" && deacon up ) || echo "Expected failure: no devcontainer.json"

# Exec before up (no running container)
( cd "$tmp" && deacon exec -- echo hello ) || echo "Expected failure: no running container"
```

## Advanced: Feature Overrides via CLI
Merge additional features or reorder installation from the command line.
```sh
FAKE=$(mktemp -d)
mkdir -p "$FAKE/.devcontainer"
cat > "$FAKE/.devcontainer/devcontainer.json" <<'JSON'
{ "name": "FeatureMerge", "image": "alpine:3.19", "features": { } }
JSON

# Add features from CLI JSON and set install order (IDs are illustrative)
deacon --workspace-folder "$FAKE" up \
  --additional-features '{"ghcr.io/devcontainers/features/common-utils:1": {"installZsh": true}}' \
  --feature-install-order 'ghcr.io/devcontainers/features/common-utils:1' \
  --prefer-cli-features \
  --skip-post-create

## Docker Unavailable Example
If you run on a system without Docker (or the daemon is stopped), Docker‑dependent commands fail with a clear message. For example, a `build`:
```sh
ND=$(mktemp -d)
cat > "$ND/Dockerfile" <<'DOCKER'
FROM alpine:3.19
DOCKER
mkdir -p "$ND/.devcontainer"
cat > "$ND/.devcontainer/devcontainer.json" <<'JSON'
{ "name": "NoDocker", "dockerFile": "Dockerfile" }
JSON

deacon --workspace-folder "$ND" build || echo "Expected failure: Docker not available"
```
```

---
All examples assume Docker is available. The "Docker Unavailable Example" demonstrates expected failure messaging when it is not.
