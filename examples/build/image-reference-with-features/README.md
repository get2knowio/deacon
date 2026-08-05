# Image Reference Build With Feature

Extends a base image by installing a local feature.

Layout: the config is `.devcontainer.json` at the example root and the local
Feature lives at `.devcontainer/features/hello/`, so the id is spelled
`./.devcontainer/features/hello` — config-relative, and contained in
`.devcontainer/` as `devcontainer-features-distribution.md` §Locally Referenced
Features requires.

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