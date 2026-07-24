# Contract: Clause Inventory + Spec Manifest

Two committed artifacts. Both strict-JSON, `deny_unknown_fields`, `camelCase`, canonical
serialization (`to_string_pretty`, 2-space indent, LF, trailing newline). Vendored prose is
byte-exact upstream Markdown, never edited in place.

## Spec manifest — `conformance/spec/<pin>/manifest.json`

```json
{
  "schemaVersion": 1,
  "revision": "rev-spec-113500f4",
  "documents": [
    { "key": "reference",       "file": "devcontainer-reference.md",              "upstreamUrl": "https://raw.githubusercontent.com/devcontainers/spec/113500f4/docs/specs/devcontainer-reference.md",              "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "json-reference",  "file": "devcontainerjson-reference.md",          "upstreamUrl": "…/devcontainerjson-reference.md",          "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "supporting-tools","file": "supporting-tools.md",                    "upstreamUrl": "…/supporting-tools.md",                    "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "image-metadata",  "file": "image-metadata.md",                      "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "lockfile",        "file": "devcontainer-lockfile.md",               "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "devcontainer-id-variable", "file": "devcontainer-id-variable.md",   "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "parallel-lifecycle", "file": "parallel-lifecycle-script-execution.md", "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "features-lifecycle-scripts", "file": "features-contribute-lifecycle-scripts.md", "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "features-user-env", "file": "features-user-env-variables.md",        "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "feature-dependencies", "file": "feature-dependencies.md",           "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "gpu-host-requirement", "file": "gpu-host-requirement.md",           "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "declarative-secrets", "file": "declarative-secrets.md",             "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "secrets-support", "file": "secrets-support.md",                     "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "features-legacy-ids", "file": "features-legacyIds-deprecated-properties.md", "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "consumer" },
    { "key": "features",        "file": "devcontainer-features.md",               "upstreamUrl": "…/devcontainer-features.md",               "sha256": "<64 hex>", "scope": "authoring" },
    { "key": "features-distribution",  "file": "devcontainer-features-distribution.md",  "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "authoring" },
    { "key": "templates",       "file": "devcontainer-templates.md",              "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "authoring" },
    { "key": "templates-distribution", "file": "devcontainer-templates-distribution.md", "upstreamUrl": "…", "sha256": "<64 hex>", "scope": "authoring" }
  ]
}
```

All **18** ratified `docs/specs/` documents at `113500f4` are mandatory (14 consumer, 4
authoring). The `features`/`templates` authoring docs are mixed and carry per-clause
`behavior-mapped` overrides for their consumer install/apply clauses (research Decision 7).

**Rules**
- `revision` MUST name an existing `rev-spec-*` record in `registry/revisions.json`
  (V14 / provenance on mismatch).
- Each `file` MUST exist in the same directory and its bytes MUST hash to `sha256`
  (verified before every parse; mismatch → `SpecFingerprintMismatch`, V14). Hashing reuses
  020's `sha256_hex`/`hex_lower`.
- `key` unique, lowercase `[a-z0-9-]+`.
- `scope` ∈ {`consumer`, `authoring`}. Only `authoring` documents may carry a document-scope
  disposition default (V13 otherwise).
- `upstreamUrl` is provenance only — no command fetches it. Exact filenames and `sha256`
  values are captured by the human vendoring task (quickstart.md); the 18 ratified
  `docs/specs/` documents above are the mandatory set (research Decision 6). The final
  document list is confirmed at vendoring; the manifest is the source of truth.

## Clause inventory — `conformance/inventory/clauses.json`

Envelope `{ schemaVersion, revision, units[] }`. Unit shape and field rules per
**data-model §2**. Contract highlights:

- `id` = `clu-<doc>-<substance-slug>-<strength>-<hash8>`; `hash8` over
  `document ‖ normalize_substance(excerpt)` — **location excluded** (Decision 2), so pure
  moves keep the ID.
- `fingerprint` = full SHA-256 of the normalized substance (the distinct fingerprint field;
  drift reads it).
- `locations[]` non-empty; each `{ heading, anchor, ordinal, excerpt }`. `excerpt` MUST be a
  verbatim substring of the vendored document under `anchor` (else `ExcerptNotFoundAtAnchor`,
  V15). Multiple locations = the same normalized obligation stated in multiple places (they
  merge into one unit).
- `strength` ∈ {`must`, `should`, `may`, `algorithm`, `io-contract`, `descriptive`}. For
  `must`/`should`/`may`, the excerpt MUST contain the corresponding RFC-2119 keyword family
  (else `StrengthKeywordMismatch`, V15). A `descriptive` unit MUST NOT contain an unqualified
  mandatory keyword.
- `testability` ∈ {`directly-testable`, `indirectly-testable`, `informative`, `ambiguous`,
  `not-applicable`}. `ambiguous` requires a per-clause classification (no document-scope cover)
  before `certify` passes.
- `units` sorted by `id`; byte-identical regeneration is the contract (V14). Generation is
  canonicalization of authored records against the pinned prose — no model, no network.

## Determinism guarantees

- No environment input (timestamp, hostname, absolute path, locale) enters either artifact.
- Markdown is read as bytes with explicit `\n` handling; CR bytes are rejected in vendored
  files at fingerprint time so cross-platform output stays byte-identical.
- `normalize_substance` and `detect_strength` are pure functions with exhaustive unit tests;
  they are the sole sources of identity, materiality, and strength verification.
