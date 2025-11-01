# Spec 003 — Features Publish Spec Compliance

## Summary
Close the implementation gap for the “features publish” subcommand so it fully matches the documented behavior. This includes semantic version tagging, collection metadata publishing, idempotent re-runs, authentication, and a spec-compliant CLI interface and outputs.

## Problem & Outcome
- Problem: Current behavior only publishes a single tag, lacks collection metadata publishing, conflates `--registry` and namespace, and isn’t idempotent. JSON output is incomplete for downstream automation.
- Desired Outcome: A compliant, user-friendly publish flow that:
  - Accepts `--registry` (default ghcr.io) and required `--namespace` separately
  - Computes and publishes semantic tags for a feature version
  - Publishes collection metadata
  - Skips already-existing tags (idempotent)
  - Supports registry authentication mechanisms
  - Emits structured JSON with published tags and digests

## Goals (What & Why)
- Align CLI flags and behavior with the specification for consistency with the ecosystem.
- Improve consumer ergonomics via semantic tags and collection discovery.
- Ensure re-runs are safe and fast (idempotent, skip existing).
- Provide structured outputs that downstream tools can trust.

## Non‑Goals (Out of Scope)
- Changing the underlying package format or feature manifest schema
- Parallel publish across multiple registries at once (can be a future enhancement)
- Non-OCI distribution mechanisms

## Actors
- Feature authors/maintainers: publish new versions and update semantic tags.
- CI engineers: automate releases, require idempotency and structured output.

## Assumptions
- OCI registry supports standard v2 APIs (tag listing, blob/manifest operations).
- Authentication is available via common mechanisms: Docker config helpers or explicit credentials.
- Feature version follows semantic versioning (major.minor.patch).

## User Scenarios & Testing
1) First publish of a new version
- Given a packaged feature at version X.Y.Z
- When the user runs `features publish [target] --registry ghcr.io --namespace owner/repo`
- Then the system publishes tags: `X`, `X.Y`, `X.Y.Z`, and `latest` (if not already present)
- And outputs JSON that includes `featureId`, `digest`, and `publishedTags`

2) Re‑publish same version (idempotent)
- Given the same feature already published with tags `X`, `X.Y`, `X.Y.Z`, `latest`
- When the user runs publish again with the same inputs
- Then the system detects existing tags and skips pushing them
- And exits successfully with a warning/log note; JSON still identifies the version and that no new tags were published

3) Invalid version input
- Given a packaged feature with a non‑semver version value
- When the user runs publish
- Then the system reports a clear validation error and exits non‑zero

4) Auth via environment / config
- Given a private registry or namespace and credentials provided via supported auth mechanisms
- When the user runs publish
- Then the operation succeeds without leaking secrets into logs

5) Collection metadata
- Given a `devcontainer-collection.json` describing the collection and its features
- When the user publishes features in a namespace
- Then the collection metadata is published to the collection ref so consumers can enumerate features

## Functional Requirements (Testable)
FR1. CLI Interface
- FR1.1: The command syntax is `features publish [target] --registry <host> --namespace <owner/repo> [--log-level <lvl>]`.
- FR1.2: `--namespace` is required; `--registry` defaults to `ghcr.io` when omitted.
- FR1.3: `--log-level` supports `info|debug|trace`.

FR2. Packaging Preconditions
- FR2.1: If packaged artifacts are not present for the target, the system packages to a temporary directory before publishing.
- FR2.2: The operation fails if no features are discovered after packaging.

FR3. Semantic Version Tagging
- FR3.1: For version `X.Y.Z`, the desired tags are `X`, `X.Y`, `X.Y.Z`, and `latest`.
- FR3.2: Version must be a valid semantic version; otherwise the command exits with a validation error.

FR4. Tag Existence & Idempotency
- FR4.1: Before pushing, the system lists currently published tags for the feature repository.
- FR4.2: Only tags not already published are pushed in the current run.
- FR4.3: If all desired tags exist, the command exits successfully and logs a skip message.

FR5. Collection Metadata Publishing
- FR5.1: The system locates `devcontainer-collection.json` in the packaged output.
- FR5.2: The collection metadata is published to the collection ref `<registry>/<namespace>`.
- FR5.3: Publish proceeds even if some features were already present (idempotent behavior applies).

FR6. Authentication & Security
- FR6.1: The system supports common registry authentication, including Docker config helpers and explicit credentials.
- FR6.2: Secrets are never logged; redaction is applied to any error or diagnostic output.

FR7. Outputs & Exit Codes
- FR7.1: Text mode logs show planned tags and which ones were newly published vs. skipped.
- FR7.2: JSON output contains at minimum: `featureId`, `digest`, and `publishedTags` for each processed feature.
- FR7.3: Exit code is `0` on success (including all‑skipped), `1` on fatal error.

## Success Criteria (Measurable)
- SC1: A new version publish emits exactly the four semantic tags when they don’t already exist (100% of runs in tests).
- SC2: Re‑publishing the same version performs no uploads and completes under 10 seconds in a local test environment.
- SC3: Invalid semantic versions are rejected with a clear message and exit code `1` in 100% of such cases in tests.
- SC4: JSON output includes `featureId`, `digest`, and `publishedTags` for each feature in 100% of test runs.
- SC5: Collection metadata is present and discoverable by consumers after publish in 100% of successful publishes.

## Key Entities & Data
- Feature: identified by `id` and `version` from the packaged manifest.
- Collection: grouping under `<registry>/<namespace>` with `devcontainer-collection.json` metadata.
- Tags: semantic aliases for a specific version (`X`, `X.Y`, `X.Y.Z`, `latest`).
- Digest: content address for the published artifact (immutable reference).

## Dependencies
- OCI Registry APIs for listing tags and publishing artifacts.
- Packaged artifacts from the “features package” workflow (or on‑the‑fly packaging when needed).

## Constraints & Edge Cases
- Network & registry transient failures should surface as actionable errors; retries may be implemented by underlying clients.
- Mixed existing and new tags in a single run must only publish the missing set.
- Namespace and IDs are validated to avoid malformed registry paths.

## Risks & Mitigations
- Risk: Inconsistent registry capabilities. Mitigation: adhere to standard v2 endpoints and give clear errors if unsupported.
- Risk: Tag conflicts during concurrent publishes. Mitigation: document sequential publish per feature; leave parallelization as future work.

## Rollout & Migration
- Introduce the `--namespace` flag as required and default `--registry` to `ghcr.io`.
- Communicate the CLI flag change and update examples and tests.

## Acceptance Tests (High‑Level)
- First publish creates tags `X`, `X.Y`, `X.Y.Z`, `latest` and outputs JSON with `publishedTags`.
- Re‑publish same version exits successfully, uploads nothing, logs skip, JSON reflects no new tags.
- Invalid version exits with error and message.
- Auth via supported mechanisms allows publish to a private registry.
- Collection metadata is pushed and discoverable.

## References
- Features Publish Subcommand Spec: `docs/subcommand-specs/features-publish/SPEC.md`
- GAP Analysis: `docs/subcommand-specs/features-publish/GAP.md`
- Feature distribution over OCI (containers.dev)
