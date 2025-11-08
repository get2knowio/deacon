# Tags with JSON Output

**User Story**: US2 - Discover published tags  
**Priority**: P1  
**Format**: JSON

## Description

Demonstrates listing published tags in JSON format for automation and programmatic access.

## Use Case

- Scripts checking for new versions
- Automated version selection in CI/CD
- Integration with dependency management tools
- Filtering tags with jq

## Prerequisites

- Network access to `ghcr.io`
- Set `DEACON_NETWORK_TESTS=1` environment variable
- Tools like `jq` for JSON processing (optional)

## Running

```bash
# Get tags as JSON
deacon features info tags ghcr.io/devcontainers/features/node --output-format json

# Filter with jq - get latest patch version
deacon features info tags ghcr.io/devcontainers/features/node --output-format json | \
  jq -r '.publishedTags[]' | grep '^1\.' | sort -V | tail -1

# Check if specific version exists
deacon features info tags ghcr.io/devcontainers/features/node --output-format json | \
  jq '.publishedTags[] | select(. == "1.2.0")'
```

## Expected Output

JSON object with single top-level key:

```json
{
  "publishedTags": [
    "1",
    "1.0.0",
    "1.1.0",
    "1.2.0",
    "1.2.1",
    "2",
    "2.0.0",
    "latest"
  ]
}
```

## Key Properties

- `publishedTags` - Array of version strings
- Tags are sorted deterministically (per FR-010)
- Empty array if no tags exist

## JSON Output Guarantees

Per spec (FR-004, FR-008):
- ✅ Output is valid JSON array
- ✅ Only JSON printed to stdout
- ✅ Keys are stable
- ✅ Tags sorted consistently
- ✅ Errors produce `{}` with exit code 1

## Success Criteria

- ✅ Output is valid JSON (`jq . <output>` succeeds)
- ✅ `publishedTags` key present
- ✅ Array contains string elements
- ✅ Exit code is 0
- ✅ No text output mixed with JSON

## Example Processing Script

```bash
#!/bin/bash
# find-version.sh - Find matching feature version

FEATURE_REF="$1"
VERSION_PATTERN="$2"

# Get tags
OUTPUT=$(deacon features info tags "$FEATURE_REF" --output-format json)

# Find matching versions
MATCHES=$(echo "$OUTPUT" | jq -r '.publishedTags[]' | grep "$VERSION_PATTERN")

if [ -n "$MATCHES" ]; then
  echo "Found matching versions:"
  echo "$MATCHES"
else
  echo "No versions match pattern: $VERSION_PATTERN"
  exit 1
fi
```

Usage:
```bash
./find-version.sh ghcr.io/devcontainers/features/node "^1\.2\."
# Output:
# Found matching versions:
# 1.2.0
# 1.2.1
```

## Related Examples

- [tags-public-feature](../tags-public-feature/) - Text format output
- [manifest-json-output](../manifest-json-output/) - JSON manifest output
