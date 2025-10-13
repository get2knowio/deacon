## Deacon CLI Examples

Each subdirectory under `examples/` is fully self‑contained: copy or `cd` into it and run the shown commands without referencing assets elsewhere in the repo.

### Index

- Build: Dockerfile builds, platform targeting, build args, secrets & SSH (`build/`)
- CLI: CLI-specific features and flags including port forwarding and custom container names (`cli/`)
- Configuration: basic, variable substitution, extends chain, and nested variables (`configuration/`)
- Container Lifecycle: lifecycle command execution, ordering, variables, skip flags, progress events, and redaction (`container-lifecycle/`)
- Doctor: environment diagnostics including host requirements and storage checks (`doctor/`)
- Docker Compose: multi-service orchestration and port events (`compose/`)
- Exec: command execution semantics covering working directory, user, TTY, and environment (`exec/`)
- Feature Management: minimal & with-options features (`feature-management/`)
- Feature System: dependencies, parallelism, caching, and lockfile support (`features/`)
- Observability: JSON logs, standardized spans, and structured fields (`observability/`)
- Registry: OCI registry operations including dry-run publish workflows (`registry/`)
- Template Management: minimal & with-options templates (`template-management/`)

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

Validate a configuration example:
```sh
cd examples/configuration/basic
deacon config validate . --json
```

Start a container with custom name:
```sh
cd examples/cli/custom-container-name
deacon up --container-name my-dev-container --skip-post-create
```

Package a feature:
```sh
cd examples/feature-management/minimal-feature
deacon features test . --json
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --json
```

Feature with options (dry-run publish):
```sh
cd examples/feature-management/feature-with-options
deacon features test . --json
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --json
deacon features publish . \
  --registry ghcr.io/example/with-options-feature \
  --dry-run --json
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

Test container lifecycle commands:
```sh
cd examples/container-lifecycle/basic
deacon read-configuration --config devcontainer.json | jq '.onCreateCommand, .postCreateCommand, .postStartCommand, .postAttachCommand'
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

Verify JSON logs with standardized spans:
```sh
cd examples/observability/json-logs
export DEACON_LOG_FORMAT=json
deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.span.name == "config.resolve")'
```

### Notes
Build examples demonstrate Dockerfile-based container builds with build arguments, platform targeting, cache control, and BuildKit features (secrets, SSH) as specified in `docs/CLI-SPEC.md` Container Build section.

Container lifecycle examples demonstrate the complete DevContainer lifecycle command execution workflow as specified in `docs/CLI-SPEC.md` Lifecycle Execution Workflow.

Doctor examples demonstrate environment diagnostics including host requirements validation (CPU, memory, storage) and real disk space checking using platform-specific APIs as specified in `docs/CLI-SPEC.md` Host Requirements section.

Exec examples demonstrate command execution semantics including working directory, user context, TTY allocation, and environment variable handling as specified in `docs/CLI-SPEC.md` Exec Command section.

Feature system examples demonstrate dependency resolution, parallel execution levels, digest-based caching, and lockfile support for reproducible builds as specified in `docs/CLI-SPEC.md` Feature Installation Workflow, Distribution & Caching sections, and lockfile specifications.

Observability examples demonstrate JSON logging, standardized tracing spans, and structured fields as specified in `docs/CLI-SPEC.md` Monitoring and Observability section.

Registry examples demonstrate OCI distribution workflows including offline-friendly dry-run publish operations for features and templates as specified in `docs/CLI-SPEC.md` Feature Distribution and Template Distribution sections.

Registry authentication examples demonstrate multiple authentication methods (environment variables, Docker config, command-line options) for push/pull operations with proper error handling and retry logic as specified in `docs/CLI-SPEC.md` OCI Registry Integration section.
