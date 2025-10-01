## Deacon CLI Examples

Each subdirectory under `examples/` is fully self‑contained: copy or `cd` into it and run the shown commands without referencing assets elsewhere in the repo.

### Index
- Build: Dockerfile builds, platform targeting, build args, secrets & SSH (`build/`)
- Configuration: basic & variable substitution examples (`configuration/`)
- Container Lifecycle: lifecycle command execution, ordering, variables, skip flags, progress events, and redaction (`container-lifecycle/`)
- Docker Compose: multi-service orchestration and port events (`compose/`)
- Exec: command execution semantics covering working directory, user, TTY, and environment (`exec/`)
- Feature Management: minimal & with-options features (`feature-management/`)
- Feature System: dependencies, parallelism, and caching (`features/`)
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

### Notes
Build examples demonstrate Dockerfile-based container builds with build arguments, platform targeting, cache control, and BuildKit features (secrets, SSH) as specified in `docs/CLI-SPEC.md` Container Build section.

Container lifecycle examples demonstrate the complete DevContainer lifecycle command execution workflow as specified in `docs/CLI-SPEC.md` Lifecycle Execution Workflow.

Exec examples demonstrate command execution semantics including working directory, user context, TTY allocation, and environment variable handling as specified in `docs/CLI-SPEC.md` Exec Command section.

Feature system examples demonstrate dependency resolution, parallel execution levels, and digest-based caching as specified in `docs/CLI-SPEC.md` Feature Installation Workflow and Distribution & Caching sections.
