## Deacon CLI Examples

Each subdirectory under `examples/` is fully self‑contained: copy or `cd` into it and run the shown commands without referencing assets elsewhere in the repo.

### Index
- Configuration: basic & variable substitution examples (`configuration/`)
- Container Lifecycle: lifecycle command execution, ordering, variables, skip flags, progress events, and redaction (`container-lifecycle/`)
- Feature Management: minimal & with-options features (`feature-management/`)
- Template Management: minimal & with-options templates (`template-management/`)

### Quick Start
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

### Notes
Container lifecycle examples demonstrate the complete DevContainer lifecycle command execution workflow as specified in `docs/CLI-SPEC.md` Lifecycle Execution Workflow.
