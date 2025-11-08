# Research — Features Publish Spec Compliance

This document consolidates decisions for Spec 003. All prior NEEDS CLARIFICATION items are resolved below.

## 1) Tag Existence & Idempotency Strategy
- Decision: List existing tags via `GET /v2/{name}/tags/list` (paginated) and compute `desired = {X, X.Y, X.Y.Z} (+ latest for stable) - existing`; push only missing. For pre‑releases, do not include or move `latest`.
- Rationale: One list request (with pagination when needed) is bandwidth‑efficient and avoids per‑tag probes. It also produces a clear planning step for logs/JSON.
- Alternatives considered:
  - HEAD manifest per desired tag: simpler code paths but incurs N HTTP requests; acceptable for small sets but worse at scale.
  - Blind push then handle 4xx/409: wastes bandwidth and yields noisier logs; contradicts idempotency goal.

## 2) Manifest/Blob Upload Flow (OCI v2)
- Decision: Follow OCI Distribution Spec v2 with correct semantics:
  - Use HEAD to check blob existence (layers and config) before upload.
  - Initiate upload with POST `/v2/{name}/blobs/uploads/`, use `Location` header for subsequent PUT with `?digest=...`.
  - Put manifest with correct mediaType; treat 201/202/204 status codes per spec.
- Rationale: Matches ecosystem expectations and our repository guidance; minimizes bytes transferred.
- Alternatives considered: Hardcoding upload URLs, using GET for existence checks — rejected per best practices.

## 3) Authentication Sources & Priority
- Decision: Support standard Docker credential resolution (config.json helpers) by default; allow explicit credentials via environment variables when provided; fail fast with actionable error when auth is required and not found.
- Rationale: Aligns with container tooling norms; avoids bespoke flags in this iteration.
- Alternatives considered: Adding CLI flags for username/password/token — defer to future if demanded by spec.

## 4) Collection Metadata Location & Media Types
- Decision: Publish `devcontainer-collection.json` as an OCI artifact under the collection repository `<registry>/<namespace>` using an artifact tag (e.g., `collection` or `latest`). Include a dedicated media type `application/vnd.devcontainer.collection+json`.
- Rationale: Enables consumers to enumerate features under a stable ref; mirrors how feature indices are commonly distributed.
- Alternatives considered: Embed in each feature repository; duplicate data and complicates discovery.

- Decision: One JSON document to stdout (when JSON mode) with:
  - `features[]` items: `featureId`, `version`, `digest`, `publishedTags`, `skippedTags`, `movedLatest` (bool), `registry`, `namespace`
  - optional `collection`: `{ digest }` when collection is published
  - `summary`: `{ features, publishedTags, skippedTags }`
- Rationale: Downstream tools need explicit lists to act upon; separation of published vs skipped preserves idempotency signals.
- Alternatives considered: Minimal output (only digest) — insufficient for automation.

## 6) Error Handling & Logging
- Decision: Treat validation errors (non‑semver) as fatal with exit code 1. Keep JSON stdout pristine; send logs to stderr with redaction for sensitive values. Provide clear messages for auth/permission and 404/5xx registry errors.
- Rationale: Matches constitution Principle V and repo guidance.
- Alternatives considered: Mixed stdout/stderr — rejected.

## 7) Performance & Retries
- Decision: No retries at our layer initially; rely on HTTP client defaults. Keep network calls minimal (list once, HEAD blobs as needed). Consider exponential backoff in core if flakes observed.
- Rationale: Keep complexity low; measure first.
- Alternatives considered: Eager retries — premature.

## 8) Testing Strategy
- Decision: Unit test semantic tag calculation and diffing; integration test CLI behavior (assert_cmd) with mock registry (`HttpClient` test double) for: first publish, re‑publish (all skipped), invalid semver, private auth path.
- Rationale: Deterministic, hermetic tests per constitution; no network access in tests.
- Alternatives considered: Live registry tests — not acceptable in CI by default.

All unknowns from the plan are now resolved sufficiently to proceed to Phase 1 design.
