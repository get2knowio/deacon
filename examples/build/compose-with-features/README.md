# Compose Service Build With Feature

Builds only the targeted Compose service (`app`) and installs a local feature.

Layout: the config is `.devcontainer.json` at the example root and the local
Feature lives at `.devcontainer/features/hello/`, so the id is spelled
`./.devcontainer/features/hello` — config-relative, and contained in
`.devcontainer/` as `devcontainer-features-distribution.md` §Locally Referenced
Features requires. (`dockerComposeFile` is unaffected: it resolves against the
**workspace folder**, so `docker-compose.yml` stays at the root.)

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