# Registry Push Example

Demonstrates pushing image tags to a registry with `--push`.

## Prerequisites
- BuildKit-enabled Docker daemon.
- Authenticated to a registry (e.g. `docker login ghcr.io`).

## Usage
```sh
cd examples/build/push
deacon build --workspace-folder . \
  --image-name ghcr.io/your-org/push-example:dev \
  --image-name ghcr.io/your-org/push-example:latest \
  --push --output-format json
```

Expected: JSON success payload lists pushed tags.

## Notes
- Demonstrates FR-003.
- Cannot combine with `--output` (FR-006).