## Deacon CLI Examples

Each subdirectory under `examples/` is fully self‑contained: copy or `cd` into it and run the shown commands without referencing assets elsewhere in the repo.

### Index

- Build: Dockerfile builds, platform targeting, build args, secrets & SSH, Compose service targeting, image reference builds, multi-tag + labels, push/export workflows, feature installation across modes, validation & error scenarios (`build/`)
- CLI: CLI-specific features and flags including port forwarding and custom container names (`cli/`)
- Configuration: basic, variable substitution, extends chain, and nested variables (`configuration/`)
- Container Lifecycle: lifecycle command execution, ordering, variables, skip flags, progress events, and redaction (`container-lifecycle/`)
- Doctor: environment diagnostics including host requirements and storage checks (`doctor/`)
- Docker Compose: multi-service orchestration and port events (`compose/`)
- Exec: command execution semantics covering working directory, user, TTY, and environment (`exec/`)
- Feature Management: minimal & with-options features (`feature-management/`)
- Feature System: dependencies, parallelism, caching, and lockfile support (`features/`)
- Feature Testing: automated test suites for features with scenarios, filtering, and JSON output (`features-test/`)
- Features Info: inspect manifests, list tags, visualize dependencies, verbose output (`features-info/`)
- Observability: JSON logs, standardized spans, and structured fields (`observability/`)
- Registry: OCI registry operations including dry-run publish workflows (`registry/`)
- Template Management: minimal & with-options templates (`template-management/`)

 - Read-Configuration: configuration reading examples (`read-configuration/`)
   - `read-configuration/basic/` — Minimal config discovery and output
   - `read-configuration/with-variables/` — Variable substitution for local env and workspace folder
   - `read-configuration/extends-chain/` — Chained `extends` across base/mid/leaf configs
   - `read-configuration/override-config/` — Apply an override with `--override-config`
   - `read-configuration/features-minimal/` — Local Feature with `--include-features-configuration`
   - `read-configuration/features-additional/` — Inject a Feature via `--additional-features`
   - `read-configuration/compose/` — Config referencing a Docker Compose file
   - `read-configuration/legacy-normalization/` — Legacy `containerEnv` normalized to `remoteEnv`
   - `read-configuration/id-labels-and-devcontainerId/` — `${devcontainerId}` via `--id-label`

### Quick Start
Build a basic Dockerfile with build args:
```sh
cd examples/build/basic-dockerfile
deacon build --workspace-folder . --build-arg FOO=BAR --output-format json
```

Build with platform targeting and no cache:
```sh
cd examples/build/platform-and-cache
deacon build --workspace-folder . --platform linux/amd64 --no-cache
```

Build with secrets (requires BuildKit):
```sh
cd examples/build/secrets-and-ssh
echo "test-secret" > /tmp/secret.txt
deacon build --workspace-folder . --secret id=foo,src=/tmp/secret.txt
```

Build from a Compose service with custom tags:
```sh
cd examples/build/compose-service-target
deacon build --workspace-folder . --image-name myapp:latest
```

Build from an image reference with labels:
```sh
cd examples/build/image-reference
deacon build --workspace-folder . --image-name myimage:latest --label "version=1.0"
```

Build with multiple tags and push to registry (requires BuildKit):
```sh
cd examples/build/basic-dockerfile
deacon build --workspace-folder . \
  --image-name myrepo/app:latest \
  --image-name myrepo/app:v1.0 \
  --push
```

Build and export to OCI archive (requires BuildKit):
```sh
cd examples/build/basic-dockerfile
deacon build --workspace-folder . --output type=oci,dest=app.tar
```

Validate a configuration example:
```sh
cd examples/configuration/basic
deacon config validate . --json
```

### Up Command Examples

Start a basic development container:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . --include-configuration | jq '.containerId'
```

Start with prebuild mode (CI/CD workflows):
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . --prebuild --include-configuration
# Outputs JSON with container ready for caching
```

Start with dotfiles installation:
```sh
cd examples/container-lifecycle/advanced
deacon up --workspace-folder . \
  --dotfiles-repository https://github.com/user/dotfiles \
  --dotfiles-install-command "./install.sh"
```

Start with additional mounts and remote environment:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . \
  --mount "type=bind,source=/tmp/cache,target=/cache" \
  --remote-env "NODE_ENV=development" \
  --remote-env "DEBUG=true"
```

Start with secrets file:
```sh
cd examples/container-lifecycle/advanced
echo "API_KEY=secret123" > /tmp/secrets.env
deacon up --workspace-folder . --secrets-file /tmp/secrets.env
# Secrets are redacted from logs automatically
```

Start with BuildKit cache:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . \
  --buildkit auto \
  --cache-from type=registry,ref=myrepo/cache \
  --cache-to type=registry,ref=myrepo/cache
```

Start with ID labels for reconnection:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . \
  --id-label project=myapp \
  --id-label environment=dev
# Later reconnect with same labels
deacon up --id-label project=myapp --id-label environment=dev --expect-existing-container
```

Start with skip flags for faster iteration:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . --skip-post-create --skip-post-attach
# Skips lifecycle hooks for faster startup
```

Start with included merged configuration:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . \
  --include-configuration \
  --include-merged-configuration | jq '.mergedConfiguration'
```

Start with custom Docker paths and data folders:
```sh
cd examples/container-lifecycle/basic
deacon up --workspace-folder . \
  --docker-path /usr/local/bin/docker \
  --container-data-folder /workspace/.devcontainer \
  --user-data-folder ~/.local/share/deacon
```

Start a container with custom name:
```sh
cd examples/cli/custom-container-name
deacon up --container-name my-dev-container --skip-post-create
```

Package a feature:
```sh
cd examples/feature-management/minimal-feature
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --json
```

Feature with options (dry-run publish):
```sh
cd examples/feature-management/feature-with-options
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --json
deacon features publish . \
  --registry ghcr.io/example/with-options-feature \
  --dry-run --json
```

Test features with comprehensive test suite:
```sh
cd examples/features-test/basic-test-suite
deacon features test .
# Or with JSON output
deacon features test . --json
```

Test specific features only:
```sh
cd examples/features-test/feature-selection
deacon features test . --features git-tools
```

Filter scenarios by name:
```sh
cd examples/features-test/scenario-filtering
deacon features test . --filter minimal
```

Test with custom base image:
```sh
cd examples/features-test/custom-environment
deacon features test . --base-image alpine:latest
```

Explore template assets:
```sh
cd examples/template-management/template-with-options
ls -1
cat devcontainer-template.json | jq '.id, .options'
```

Apply a template with custom options:
```sh
cd examples/template-management/templates-apply
mkdir -p /tmp/my-project
deacon templates apply ../template-with-options \
  --output /tmp/my-project \
  --option customName=my-app \
  --option debugMode=true
```

View template metadata:
```sh
cd examples/template-management/metadata-and-docs
deacon templates metadata ../template-with-options | jq '.options | keys'
```

Generate template documentation:
```sh
cd examples/template-management/metadata-and-docs
mkdir -p /tmp/docs
deacon templates generate-docs ../template-with-options --output /tmp/docs
cat /tmp/docs/README-template.md
```

Test dry-run publish workflows for features and templates:
```sh
cd examples/registry/dry-run-publish

# Dry-run publish a feature
cd feature
deacon features publish . \
  --registry ghcr.io/example/my-feature \
  --dry-run --json 2>/dev/null | jq '.'

# Dry-run publish a template
cd ../template
deacon templates publish . \
  --registry ghcr.io/example/my-template \
  --dry-run 2>/dev/null | jq '.'
```

Run doctor command for system diagnostics:
```sh
cd examples/doctor/host-requirements
deacon doctor --workspace-folder .
# Or for JSON output
deacon doctor --workspace-folder . --json | jq '.disk_space'
```

Test container lifecycle commands (with full up execution):
```sh
cd examples/container-lifecycle/basic
# First, inspect the configuration
deacon read-configuration --config devcontainer.json | jq '.onCreateCommand, .postCreateCommand, .postStartCommand, .postAttachCommand'

# Then execute with up command to see lifecycle in action
deacon up --workspace-folder . --include-configuration
```

Explore lifecycle execution order:
```sh
cd examples/container-lifecycle/execution-order
deacon read-configuration --config devcontainer.json | jq -r '
  "1. onCreate: " + (.onCreateCommand | tostring),
  "2. postCreate: " + (.postCreateCommand | tostring),
  "3. postStart: " + (.postStartCommand | tostring),
  "4. postAttach: " + (.postAttachCommand | tostring)
'
```

Test configuration extends chain:
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json --include-merged-configuration | jq '.__meta.layers'
```

Test nested variable substitution:
```sh
cd examples/configuration/nested-variables
deacon config substitute --config devcontainer.json --dry-run | jq '.configuration.containerEnv'
```

Test skip flags behavior:
```sh
cd examples/container-lifecycle/non-blocking-and-skip
deacon read-configuration --config devcontainer.json | jq '{
  onCreate: .onCreateCommand,
  postCreate: .postCreateCommand,
  postStart: .postStartCommand,
  postAttach: .postAttachCommand
}'
```

Analyze progress events structure:
```sh
cd examples/container-lifecycle/progress-events
deacon read-configuration --config devcontainer.json | jq '.postCreateCommand'
# Shows named commands that will generate stable command IDs
```

Verify redaction configuration:
```sh
cd examples/container-lifecycle/redaction
deacon read-configuration --config devcontainer.json | jq '.containerEnv'
# Shows environment variables (including sensitive ones that will be redacted)
```

Start a multi-service compose environment:
```sh
cd examples/compose/multiservice-basic
deacon up
deacon exec sh -lc 'echo ok'
```

Capture port events from a compose service:
```sh
cd examples/compose/port-events
deacon up --ports-events 2>&1 | grep "PORT_EVENT:"
```

Test exec command semantics:
```sh
cd examples/exec/semantics
deacon up
deacon exec sh -lc 'pwd'  # Should output /wsp
deacon exec --env FOO=BAR sh -lc 'echo $FOO'  # Should output BAR
deacon down
```

View feature dependency resolution and installation plan:
```sh
cd examples/features/dependencies-and-ordering
deacon features plan --config devcontainer.json --json
```

Demonstrate parallel feature installation:
```sh
cd examples/features/parallel-install-demo
deacon features plan --config devcontainer.json --json | jq '.levels'
```

Explore feature caching:
```sh
cd examples/features/cache-reuse-hint
RUST_LOG=debug deacon read-configuration --config devcontainer.json
```

Examine lockfile structure and path derivation:
```sh
cd examples/features/lockfile-demo
cat devcontainer-lock.json | jq '.features | keys'
cat devcontainer-lock.json | jq '.features["ghcr.io/devcontainers/features/node:1"]'
```

Inspect feature manifest and canonical ID:
```sh
cd examples/features-info/manifest-public-registry
# Requires: export DEACON_NETWORK_TESTS=1
deacon features info manifest ghcr.io/devcontainers/features/node:1
# For JSON output
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json | jq '.canonicalId'
```

List published tags for a feature:
```sh
cd examples/features-info/tags-public-feature
# Requires: export DEACON_NETWORK_TESTS=1
deacon features info tags ghcr.io/devcontainers/features/node
# For JSON output
deacon features info tags ghcr.io/devcontainers/features/node --output-format json | jq '.publishedTags | length'
```

Visualize feature dependencies as Mermaid graph:
```sh
cd examples/features-info/dependencies-simple
deacon features info dependencies ./my-feature
# Copy output and paste into https://mermaid.live/
```

Get complete feature info in verbose mode:
```sh
cd examples/features-info/verbose-text-output
# Requires: export DEACON_NETWORK_TESTS=1
deacon features info verbose ghcr.io/devcontainers/features/node:1
# For JSON (manifest + tags, no dependencies)
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json | jq 'keys'
```

Inspect local feature manifest:
```sh
cd examples/features-info/manifest-local-feature
deacon features info manifest ./sample-feature
# For JSON output
deacon features info manifest ./sample-feature --output-format json | jq '.canonicalId'
# Output: null (local features have no OCI digest)
```

Verify JSON logs with standardized spans:
```sh
cd examples/observability/json-logs
export DEACON_LOG_FORMAT=json
deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.span.name == "config.resolve")'
```

### Notes
Build examples demonstrate Dockerfile-based container builds with build arguments, platform targeting, cache control, and BuildKit features (secrets, SSH) as specified in `docs/subcommand-specs/*/SPEC.md` Container Build section. Additional examples showcase Compose service targeting (`compose-service-target/`), image reference builds (`image-reference/`), multi-tag support with `--image-name`, registry push with `--push`, and OCI archive export with `--output`. See `docs/subcommand-specs/build/SPEC.md` for complete build parity documentation.
Specific build capability example directories (under `examples/build/`):

- `basic-dockerfile/` – Minimal Dockerfile build
- `platform-and-cache/` – Platform selection & cache control
- `secrets-and-ssh/` – BuildKit secrets & SSH forwarding
- `compose-service-target/` – Targeted Docker Compose service image
- `image-reference/` – Extending a referenced base image
- `multi-tags-and-labels/` – Multiple `--image-name` tags & custom `--label` injection
- `output-archive/` – Exporting image as OCI archive via `--output`
- `push/` – Publishing tags to a registry with `--push` (BuildKit required)
- `push-output-conflict/` – Demonstrates mutual exclusion of `--push` and `--output`
- `dockerfile-with-features/` – Feature install during Dockerfile build
- `image-reference-with-features/` – Feature install atop base image
- `compose-with-features/` – Feature install with Compose service build
- `compose-unsupported-flags/` – Demonstrates pre-build rejection of unsupported flags
- `compose-missing-service/` – Error when referenced service does not exist
- `buildkit-gated-feature/` – Example of feature requiring BuildKit-only capability
- `invalid-config-name/` – Validation failure for incorrect config filename
- `duplicate-tags/` – Duplicate tag validation error scenario
- `unwritable-output/` – Failing fast on unwritable `--output` destination


Container lifecycle examples demonstrate the complete DevContainer lifecycle command execution workflow as specified in `docs/subcommand-specs/*/SPEC.md` Lifecycle Execution Workflow. The `up` command now has full parity with the specification including: JSON output contract, all CLI flags (workspace/config/lifecycle/mount/env/cache/buildkit/dotfiles/security), validation rules, updateContentCommand execution, prebuild mode, ID labels, secrets handling with redaction, image metadata merging, feature-driven builds, compose parity (mount conversion, profiles, remote-env), UID/security options, and data folder management. See `docs/subcommand-specs/up/GAP.md` for complete implementation status (~95% specification compliance).

Doctor examples demonstrate environment diagnostics including host requirements validation (CPU, memory, storage) and real disk space checking using platform-specific APIs as specified in `docs/subcommand-specs/*/SPEC.md` Host Requirements section.

Exec examples demonstrate command execution semantics including working directory, user context, TTY allocation, and environment variable handling as specified in `docs/subcommand-specs/*/SPEC.md` Exec Command section.

Feature system examples demonstrate dependency resolution, parallel execution levels, digest-based caching, and lockfile support for reproducible builds as specified in `docs/subcommand-specs/*/SPEC.md` Feature Installation Workflow, Distribution & Caching sections, and lockfile specifications.

Feature testing examples demonstrate comprehensive test suites for features including autogenerated tests, scenarios with custom images, duplicate/idempotence checks, global scenarios, scenario filtering, JSON output, and custom test environments as specified in `docs/subcommand-specs/features-test/SPEC.md`.

Features Info examples demonstrate the four info modes (manifest, tags, dependencies, verbose) with both text and JSON output formats, including local feature support, error handling, and edge cases as specified in `docs/subcommand-specs/features-info/SPEC.md`.

Observability examples demonstrate JSON logging, standardized tracing spans, and structured fields as specified in `docs/subcommand-specs/*/SPEC.md` Monitoring and Observability section.

Registry examples demonstrate OCI distribution workflows including offline-friendly dry-run publish operations for features and templates as specified in `docs/subcommand-specs/*/SPEC.md` Feature Distribution and Template Distribution sections.

Registry authentication examples demonstrate multiple authentication methods (environment variables, Docker config, command-line options) for push/pull operations with proper error handling and retry logic as specified in `docs/subcommand-specs/*/SPEC.md` OCI Registry Integration section.
