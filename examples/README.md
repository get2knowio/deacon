## Deacon CLI Examples

Each subdirectory under `examples/` is fully self‑contained: copy or `cd` into it and run the shown commands without referencing assets elsewhere in the repo.

### Index
- Configuration: basic & variable substitution examples (`configuration/`)
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

### Notes
- Container lifecycle scenario examples will be added once the corresponding CLI workflows (see `docs/CLI-SPEC.md` Lifecycle Execution Workflow) are stabilized.
