# Compose Missing Service Example

Triggers the documented error when the configured service is not defined in the Compose file.

## Usage (Expect Error)
```sh
cd examples/build/compose-missing-service
deacon build --workspace-folder . --image-name myorg/compose-missing:latest
```

Expected: fast-fail stating the configured service was not found (FR-010).