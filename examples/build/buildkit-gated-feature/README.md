# BuildKit-Gated Feature Example

Demonstrates a feature that should fail fast if BuildKit is not available.

Layout: the config is `.devcontainer.json` at the example root and the local
Feature lives at `.devcontainer/features/buildkit-only/`, so the id is spelled
`./.devcontainer/features/buildkit-only` — config-relative, and contained in
`.devcontainer/` as `devcontainer-features-distribution.md` §Locally Referenced
Features requires.

## Usage
```sh
cd examples/build/buildkit-gated-feature
deacon build --workspace-folder . --image-name myorg/buildkit-gated:latest --output-format json
```

## Expected
- If BuildKit disabled: error with documented gating message (FR-011).
- If BuildKit enabled: feature installs and image builds.

## Notes
`buildKitRequired` flag is illustrative; actual gating logic depends on implementation.