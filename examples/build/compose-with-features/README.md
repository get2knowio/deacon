# Compose Service Build With Feature

Builds only the targeted Compose service (`app`) and installs a local feature.

## Usage
```sh
cd examples/build/compose-with-features
deacon build --workspace-folder . --image-name myorg/compose-feature:latest --output-format json
```

## Verify
```sh
# The feature's install.sh writes /hello.txt into the built image.
docker run --rm myorg/compose-feature:latest cat /hello.txt
```

## Notes
- Demonstrates FR-010 (service targeting) & FR-008 (feature install).
- The target service's shape (`build:` here, or `image:`) is resolved and a
  feature-extended image is built; `--image-name` tags that final image.
- Unsupported flags (e.g. `--push`, `--output`) should be rejected per spec.