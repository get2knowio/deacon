# Dockerfile Build With Local Feature

Demonstrates feature installation when building from a Dockerfile.

Layout: the config is `.devcontainer.json` at the example root and the local
Feature lives at `.devcontainer/features/hello/`, so the id is spelled
`./.devcontainer/features/hello` — config-relative, and contained in
`.devcontainer/` as `devcontainer-features-distribution.md` §Locally Referenced
Features requires.

## Usage
```sh
cd examples/build/dockerfile-with-features
deacon build --workspace-folder . --image-name myorg/feat-dockerfile:latest --output-format json
```

After build, verify feature artifact:
```sh
docker run --rm myorg/feat-dockerfile:latest cat /hello.txt
```

## Notes
- Demonstrates FR-008 (feature install in Dockerfile mode).