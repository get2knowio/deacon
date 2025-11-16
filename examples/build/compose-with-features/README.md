# Compose Service Build With Feature

Builds only the targeted Compose service (`app`) and installs a local feature.

## Usage
```sh
cd examples/build/compose-with-features
deacon build --workspace-folder . --image-name myorg/compose-feature:latest --output-format json
```

## Notes
- Demonstrates FR-010 (service targeting) & FR-008 (feature install).
- Unsupported flags (e.g. `--push`, `--output`) should be rejected per spec.