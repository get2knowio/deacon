# Verbose Mode - JSON Output

**User Story**: US4 - Combined verbose view  
**Priority**: P2  
**Format**: JSON

## Description

Demonstrates verbose mode with JSON output. Unlike text mode, JSON verbose mode only includes manifest, canonicalId, and publishedTags (no dependency graph).

## Use Case

- Automated feature discovery and caching
- CI/CD pipelines collecting feature metadata
- Tools requiring both manifest and tags in one request
- Reducing API calls for complete feature info

## Prerequisites

- Network access to `ghcr.io`
- Set `DEACON_NETWORK_TESTS=1` environment variable
- Tools like `jq` for JSON processing (optional)

## Running

```bash
# Get comprehensive feature info as JSON
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json

# Extract specific fields
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json | jq '.canonicalId'
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json | jq '.publishedTags | length'
```

## Expected Output

JSON object combining manifest and tags data:

```json
{
  "manifest": {
    "schemaVersion": 2,
    "config": {
      "mediaType": "application/vnd.devcontainers.config.v0+json",
      "digest": "sha256:...",
      "size": 1234
    },
    "layers": [...]
  },
  "canonicalId": "ghcr.io/devcontainers/features/node@sha256:abc123...",
  "publishedTags": [
    "1",
    "1.0.0",
    "1.1.0",
    "latest"
  ]
}
```

## Key Differences from Text Mode

Per spec (FR-006, US4 Acceptance Scenario 2):
- ✅ Includes: `manifest`, `canonicalId`, `publishedTags`
- ❌ Excludes: Dependency graph (Mermaid syntax is text-only)

This is intentional - JSON mode focuses on machine-readable data.

## Partial Failure Behavior

Per spec (US4 Acceptance Scenarios 3-4):

If any sub-mode fails (manifest, tags, or dependencies):
- Include successfully retrieved fields
- Add `errors` object with failure details
- Exit with code 1

Example partial failure (tags fetch failed):
```json
{
  "manifest": {...},
  "canonicalId": "ghcr.io/devcontainers/features/node@sha256:abc123...",
  "errors": {
    "tags": "Failed to list tags: timeout after 10s"
  }
}
```

Exit code: **1** (even though partial data returned)

## JSON Output Guarantees

Per spec (FR-006, FR-008):
- ✅ Always valid JSON
- ✅ `canonicalId` always present (null for local features)
- ✅ Keys stable across runs
- ✅ Only JSON on stdout (logs to stderr)
- ✅ Partial failure includes `errors` object + exit 1

## Success Criteria

- ✅ Output is valid JSON
- ✅ Contains `manifest`, `canonicalId`, `publishedTags`
- ✅ Does NOT contain dependency graph
- ✅ Exit code 0 on complete success
- ✅ Exit code 1 with `errors` on partial failure

## Example Processing Script

```bash
#!/bin/bash
# feature-summary.sh - Generate feature summary

FEATURE_REF="$1"

# Get verbose info
OUTPUT=$(deacon features info verbose "$FEATURE_REF" --output-format json)
EXIT_CODE=$?

# Extract data
CANONICAL_ID=$(echo "$OUTPUT" | jq -r '.canonicalId')
TAG_COUNT=$(echo "$OUTPUT" | jq '.publishedTags | length')
HAS_ERRORS=$(echo "$OUTPUT" | jq 'has("errors")')

echo "Feature: $FEATURE_REF"
echo "Canonical ID: $CANONICAL_ID"
echo "Published Tags: $TAG_COUNT"

if [ "$HAS_ERRORS" = "true" ]; then
  echo "Warnings: $(echo "$OUTPUT" | jq -r '.errors | to_entries | map("\(.key): \(.value)") | join(", ")')"
  echo "Status: Partial"
else
  echo "Status: Complete"
fi

exit $EXIT_CODE
```

## Related Examples

- [verbose-text-output](../verbose-text-output/) - Text format (includes dependencies)
- [manifest-json-output](../manifest-json-output/) - Manifest only JSON
- [tags-json-output](../tags-json-output/) - Tags only JSON
