# BuildKit-Gated Feature Example

Demonstrates a feature that should fail fast if BuildKit is not available.

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