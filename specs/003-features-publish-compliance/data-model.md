# Data Model — Features Publish

## Entities

- FeatureArtifact
  - id: string (e.g., "owner/repo/featureId")
  - version: SemVer (X.Y.Z)
  - digest: string (sha256:...)
  - packaged_path: Path (where artifacts are staged)

- PublishPlan
  - registry: string (default: ghcr.io)
  - namespace: string (e.g., owner/repo)
  - desired_tags: [string] (computed: X, X.Y, X.Y.Z, latest)
  - existing_tags: [string] (queried via /tags/list)
  - to_publish: [string] (desired − existing)

- PublishResult
  - featureId: string
  - version: string
  - digest: string
  - publishedTags: [string]
  - skippedTags: [string]
  - movedLatest: boolean
  - registry: string
  - namespace: string

- PublishSummary
  - features: number (count of features processed)
  - publishedTags: number (total across all features)
  - skippedTags: number (total across all features)

- CollectionResult (optional)
  - digest: string

- CollectionMetadata
  - raw: JSON (contents of devcontainer-collection.json)
  - mediaType: string (application/vnd.devcontainer.collection+json)
  - ref: string (<registry>/<namespace>:collection)

## Relationships

- FeatureArtifact 1..1 → PublishPlan: a plan is created for a single feature artifact.
- PublishPlan 1..N → PublishResult: results generated per feature (N≥1 when packaging discovers multiple).
- Namespace aggregates many FeatureArtifact repositories and one CollectionMetadata ref.

## Validation Rules

- version MUST be valid SemVer (FR3.2). Reject otherwise.
- namespace MUST be non-empty and conform to `<owner>/<repo>`.
- desired_tags derived strictly from `version`.
- JSON output MUST include `featureId`, `digest`, `publishedTags` per feature (FR7.2).
- `latest` MUST NOT be created/moved for pre‑release versions; `movedLatest` MUST be false in such cases.

## State Transitions

- DISCOVERED → PACKAGED → PLANNED → PUBLISHED (or SKIPPED)
- PACKAGED: artifacts staged or generated on the fly (FR2.x)
- PLANNED: `existing_tags` collected and `to_publish` computed
- PUBLISHED: successful uploads and manifest push per tag; update `publishedTags`
- SKIPPED: tag existed; recorded in `skippedTags`
  - On stable release when `latest` advances, set `movedLatest = true`; otherwise `false`.
