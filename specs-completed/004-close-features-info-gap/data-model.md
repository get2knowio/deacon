# Data Model — Features Info

This feature is read-only. Entities and shapes define inputs and outputs for CLI modes.

## Entities

### FeatureReference
- Description: Identifier for a feature. Local path or remote OCI ref.
- Shape (string):
  - Local: `./path/to/feature`
  - Remote: `<registry>/<namespace>/<name>[:tag]`
- Validation:
  - Must parse into registry + path for remote.
  - Tag optional; digest may be present in canonical forms.

### OCIManifest (subset)
- Description: Manifest object from OCI registry. Minimal fields used for canonical digest.
- Fields:
  - `schemaVersion: number`
  - `mediaType: string`
  - `config: { mediaType: string, digest: string, size: number }`
  - `layers: Array<{ mediaType: string, digest: string, size: number }>`
- Validation: `schemaVersion` in [1,2]; digests are algorithm-prefixed (e.g., `sha256:...`).

### PublishedTags
- Description: Array of available tags for a feature repository.
- Shape: `string[]`
- Validation:
  - Non-empty on success path; empty ⇒ error for `tags` mode.
  - Sorted by registry order when provided; otherwise ascending lexicographic.

### DependencyGraph (text)
- Description: Representation derived from feature metadata relationships.
- Shape: Mermaid `graph TD` text (no JSON).
- Validation: Mermaid syntax parses on mermaid.live; includes nodes and edges for `dependsOn` / `installsAfter`.

### VerboseJson
- Description: Combined output for `verbose` mode in JSON.
- Fields:
  - `manifest: OCIManifest` (optional if fetch fails)
  - `canonicalId: string | null` (always present; null for local)
  - `publishedTags: string[]` (optional if fetch fails)
  - `errors?: { manifest?: string; tags?: string; dependencies?: string }`
- Validation: If any `errors` present ⇒ process exits with code 1.

## State Transitions
- None (read-only). Each mode fetches data independently; `verbose` aggregates.
