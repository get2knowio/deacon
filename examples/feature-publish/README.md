# Feature Publish Examples

This folder contains a set of examples demonstrating `deacon features publish` behavior and exercising the features-publish spec. Each example is self-contained and includes a `README.md` with instructions.

Examples:
- `single-feature-basic` — package & dry-run publish of a single feature (semantic tags expected)
- `multi-feature-collection` — multiple features with collection metadata publishing
- `idempotent-republish` — shows safe re-run behavior (skip existing tags)
- `auth-local-registry` — demonstrates using `DEVCONTAINERS_OCI_AUTH` for local registry auth
- `invalid-version` — demonstrates validation error for non-semver version

Run examples by `cd` into an example folder and following its README.
