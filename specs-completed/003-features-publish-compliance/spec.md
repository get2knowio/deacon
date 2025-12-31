# Spec 003 — Features Publish Spec Compliance

## Summary
Close the implementation gap for the “features publish” subcommand so it fully matches the documented behavior. This includes semantic version tagging, collection metadata publishing, idempotent re-runs, authentication, and a spec-compliant CLI interface and outputs.

## Clarifications
### Session 2025-11-01
- Q: What are the “latest” tag movement rules during publish? → A: Move latest for stable (non‑pre‑release) versions; do not move it for pre‑releases.
- Q: What is the JSON output top‑level structure for publish results? → A: A single root object with `features: []`, optional `collection`, and a `summary` section. Each feature result includes: `featureId`, `version`, `digest`, `publishedTags`, `skippedTags`, `movedLatest`, `registry`, `namespace`.

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
- Then the system publishes tags: `X`, `X.Y`, `X.Y.Z`, and `latest` (stable versions only; pre‑releases do not move `latest`)
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
  - Error contract: exit code `1`; no JSON body on stdout in JSON mode; an actionable message is written to stderr:
    - Message template: `No features found to publish in "<target>" (after packaging).`
  - Validation occurs before any network calls.

FR3. Semantic Version Tagging
- FR3.1: For version `X.Y.Z`, the desired tags are `X`, `X.Y`, `X.Y.Z`; add `latest` for stable (non‑pre‑release) versions only.
- FR3.2: Version must be a valid semantic version; otherwise the command exits with a validation error.
- FR3.3: Pre‑release versions (e.g., with identifiers like `-rc`, `-beta`) MUST NOT create or move the `latest` tag.

FR4. Tag Existence & Idempotency
- FR4.1: Before pushing, the system lists currently published tags for the feature repository.
- FR4.2: Only tags not already published are pushed in the current run.
- FR4.3: If all desired tags exist, the command exits successfully and logs a skip message.

FR5. Collection Metadata Publishing
- FR5.1: The system locates `devcontainer-collection.json` in the packaged output.
- FR5.2: The collection metadata is published under the collection reference `<registry>/<namespace>:collection` using media type `application/vnd.devcontainer.collection+json`. The artifact MUST be addressed as a distinct tag (`:collection`) at the collection repository root.
- FR5.3: Publish proceeds even if some features were already present (idempotent behavior applies).

FR6. Authentication & Security
- FR6.1: The system supports common registry authentication, including Docker config helpers and explicit credentials.
- FR6.2: Secrets are never logged; redaction is applied to any error or diagnostic output.

FR7. Outputs & Exit Codes
- FR7.1: Text mode logs show planned tags and which ones were newly published vs. skipped.
- FR7.2: JSON output is a single root object with the following structure:
  - FR7.2.1: `features` (array) — each item includes:
    - `featureId` (string)
    - `version` (string)
    - `digest` (string)
    - `publishedTags` (array of strings)
    - `skippedTags` (array of strings)
    - `movedLatest` (boolean)
    - `registry` (string)
    - `namespace` (string)
  - FR7.2.2: `collection` (object, optional) — if collection metadata is published, includes:
    - `digest` (string)
  - FR7.2.3: `summary` (object) — includes totals:
    - `features` (number)
    - `publishedTags` (number)
    - `skippedTags` (number)
- FR7.3: Exit code is `0` on success (including all‑skipped), `1` on fatal error.
 - FR7.4: Stdout/stderr separation (Constitution V):
   - JSON mode (`--output json`): stdout contains only the single JSON document; all logs/diagnostics go to stderr.
   - On fatal errors: stdout is empty; the error message is written to stderr.

Example (stable release):

```json
{
  "features": [
    {
      "featureId": "ghcr.io/owner/repo/my-feature",
      "version": "1.2.3",
      "digest": "sha256:...",
      "publishedTags": ["1", "1.2", "1.2.3", "latest"],
      "skippedTags": [],
      "movedLatest": true,
      "registry": "ghcr.io",
      "namespace": "owner/repo"
    }
  ],
  "collection": { "digest": "sha256:..." },
  "summary": { "features": 1, "publishedTags": 4, "skippedTags": 0 }
}
```

## Success Criteria (Measurable)
- SC1: A new stable (non‑pre‑release) version publish emits exactly the four semantic tags when they don’t already exist (100% of runs in tests).
- SC2: Re‑publishing the same version performs no uploads and completes under 10 seconds in a local test environment.
- SC3: Invalid semantic versions are rejected with a clear message and exit code `1` in 100% of such cases in tests.
- SC4: JSON output is a root object with `features[]` (containing `featureId`, `version`, `digest`, `publishedTags`, `skippedTags`, `movedLatest`, `registry`, `namespace`), optional `collection.digest`, and a `summary` with totals in 100% of test runs.
- SC5: Collection metadata is present and discoverable by consumers after publish in 100% of successful publishes.
- SC6: Publishing a pre‑release version does not update the `latest` tag in 100% of such cases in tests.

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
- Pre‑release versions must never update or create the `latest` tag.
 - Collection artifact specifics:
   - Ref and tag: `<registry>/<namespace>:collection`
   - Media type: `application/vnd.devcontainer.collection+json`

## Risks & Mitigations
- Risk: Inconsistent registry capabilities. Mitigation: adhere to standard v2 endpoints and give clear errors if unsupported.
- Risk: Tag conflicts during concurrent publishes. Mitigation: document sequential publish per feature; leave parallelization as future work.

## Rollout & Migration
- Introduce the `--namespace` flag as required and default `--registry` to `ghcr.io`.
- Communicate the CLI flag change and update examples and tests.

## Acceptance Tests (High‑Level)
- First publish of a stable version creates tags `X`, `X.Y`, `X.Y.Z`, `latest` and outputs JSON with the specified root object shape (includes `features[]`, optional `collection`, and `summary`).
- Re‑publish same version exits successfully, uploads nothing, logs skip, JSON reflects no new tags.
- Invalid version exits with error and message.
- Auth via supported mechanisms allows publish to a private registry.
- Collection metadata is pushed and discoverable.
  - JSON includes `collection.digest` when collection is published.
- Pre‑release publish creates only `X`, `X.Y`, `X.Y.Z` and does not move or create `latest`.

## References
- Features Publish Subcommand Spec: `docs/subcommand-specs/features-publish/SPEC.md`
- GAP Analysis: `docs/subcommand-specs/features-publish/GAP.md`
- Feature distribution over OCI (containers.dev)
