# Manifest with JSON Output

**User Story**: US1 - Inspect manifest and canonical ID  
**Priority**: P1  
**Format**: JSON

## Description

Demonstrates fetching feature manifest in JSON format for automation and CI/CD pipelines. JSON output is ideal for parsing by scripts and tools.

## Use Case

- CI/CD pipelines extracting digest for verification
- Automation scripts processing feature metadata
- Integration with other tools requiring structured data

## Prerequisites

- Network access to `ghcr.io`
- Set `DEACON_NETWORK_TESTS=1` environment variable
- Tools like `jq` for JSON processing (optional)

## Running

```bash
# Get manifest as JSON
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json

# Parse with jq
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json | jq '.canonicalId'

# Extract digest only
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json | jq -r '.canonicalId' | cut -d'@' -f2
```

## Expected Output

JSON object with two top-level keys:

```json
{
  "manifest": {
    "schemaVersion": 2,
    "config": {
      "mediaType": "application/vnd.devcontainers.config.v0+json",
      "digest": "sha256:...",
      "size": 1234
    },
    "layers": [
      {
        "mediaType": "application/vnd.devcontainers.layer.v1+tar",
        "digest": "sha256:...",
        "size": 5678
      }
    ]
  },
  "canonicalId": "ghcr.io/devcontainers/features/node@sha256:abc123..."
}
```

## Key Properties

- `manifest` - Complete OCI manifest object
- `canonicalId` - Full reference with SHA256 digest (null for local features)

## JSON Output Guarantees

Per spec (FR-002, FR-008):
- ✅ Output is valid JSON
- ✅ Only JSON printed to stdout (logs go to stderr)
- ✅ Keys are stable and always present
- ✅ `canonicalId` is `null` for local features
- ✅ Errors produce `{}` with exit code 1

## Success Criteria

- ✅ Output is valid JSON (`jq . <output>` succeeds)
- ✅ Both `manifest` and `canonicalId` keys present
- ✅ `canonicalId` includes `@sha256:` prefix
- ✅ Exit code is 0
- ✅ No text output mixed with JSON

## Example Processing Script

```bash
#!/bin/bash
# verify-feature.sh - Verify a feature's canonical ID

FEATURE_REF="$1"
EXPECTED_DIGEST="$2"

# Get manifest
OUTPUT=$(deacon features info manifest "$FEATURE_REF" --output-format json)

# Extract digest
ACTUAL_DIGEST=$(echo "$OUTPUT" | jq -r '.canonicalId' | cut -d'@' -f2)

# Compare
if [ "$ACTUAL_DIGEST" = "$EXPECTED_DIGEST" ]; then
  echo "✓ Feature digest verified: $ACTUAL_DIGEST"
  exit 0
else
  echo "✗ Digest mismatch!"
  echo "  Expected: $EXPECTED_DIGEST"
  echo "  Actual:   $ACTUAL_DIGEST"
  exit 1
fi
```

## Related Examples

- [manifest-public-registry](../manifest-public-registry/) - Text format output
- [manifest-local-feature](../manifest-local-feature/) - Local feature with null canonicalId
