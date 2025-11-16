# Image Reference Build With Feature

Extends a base image (`alpine:3.19`) by installing a local feature.

## Usage
```sh
cd examples/build/image-reference-with-features
deacon build --workspace-folder . --image-name myorg/feat-image-ref:latest --output-format json
```

Verify:
```sh
docker run --rm myorg/feat-image-ref:latest cat /hello.txt
```

## Notes
- Demonstrates FR-009 & FR-008.