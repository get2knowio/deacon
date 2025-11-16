# Build Subcommand

- Executive Summary: Builds a Dev Container image from a devcontainer configuration by resolving configuration, applying Features, embedding devcontainer metadata, and producing one or more tagged images. Supports BuildKit for multi-arch, push, export, and cache operations. Compose configurations are supported with constraints (no platform/push/output/cache-to).

- Common Use Cases:
  - Prebuild image for faster `up`: `devcontainer build --workspace-folder . --image-name org/app:dev`
  - Multi-arch with BuildKit: `devcontainer build --workspace-folder . --platform linux/amd64 --image-name org/app:amd64`
  - Export OCI archive: `devcontainer build --workspace-folder . --output type=oci,dest=output.tar`
  - Compose service retag: `devcontainer build --workspace-folder . --image-name org/app:compose`

- Documents:
  - SPEC: docs/subcommand-specs/build/SPEC.md
  - DIAGRAMS: docs/subcommand-specs/build/DIAGRAMS.md
  - DATA STRUCTURES: docs/subcommand-specs/build/DATA-STRUCTURES.md

- Implementation Checklist:
  - Parse CLI; enforce `--push` vs `--output` exclusivity; gate BuildKit-only flags.
  - Resolve devcontainer.json (or Compose) relative to `--workspace-folder` or `--config`.
  - Validate Features via control manifest; merge `--additional-features` over config.
  - For Dockerfile/Image configs: build or extend image; add labels; tag `--image-name`.
  - For Compose configs: generate override and derive original image name; tag `--image-name` if provided.
  - Stream logs to stderr; emit JSON result on stdout; return 0/1 exit codes.

