# Local Feature - Manifest Mode Only

**Edge Case**: Local features only support manifest mode  
**Format**: Both text and JSON

## Description

Demonstrates the limitation that local features only support `manifest` mode. Other modes (`tags`, `dependencies`, `verbose`) require registry access and will fail gracefully.

## Use Case

- Understanding local feature limitations
- Feature development workflow
- Error handling for unsupported operations

## Prerequisites

None - works entirely offline

## Supported: Manifest Mode

```bash
cd examples/features-info/local-feature-only-manifest

# Text output
deacon features info manifest ./local-feature

# JSON output
deacon features info manifest ./local-feature --output-format json
```

### Text Output

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Manifest                                                                  ║
╚═══════════════════════════════════════════════════════════════════════════╝
{
  "id": "local-dev-feature",
  "version": "0.1.0",
  ...
}

╔═══════════════════════════════════════════════════════════════════════════╗
║ Canonical Identifier                                                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
(local feature)
```

### JSON Output

```json
{
  "manifest": {
    "id": "local-dev-feature",
    "version": "0.1.0",
    ...
  },
  "canonicalId": null
}
```

**Key**: `canonicalId` is always `null` for local features (no OCI digest available).

## Unsupported: Tags Mode

```bash
deacon features info tags ./local-feature
```

**Error** (text mode):
```
Error: Mode 'tags' requires registry access. Local features only support 'manifest' mode.
```

**Error** (JSON mode):
```json
{}
```

Exit code: **1**

## Unsupported: Dependencies Mode

```bash
deacon features info dependencies ./local-feature
```

**Error** (text mode):
```
Error: Mode 'dependencies' requires registry access. Local features only support 'manifest' mode.
```

**Error** (JSON mode):
```json
{}
```

Exit code: **1**

## Unsupported: Verbose Mode

```bash
deacon features info verbose ./local-feature
```

**Error** (text mode):
```
Error: Mode 'verbose' requires registry access. Local features only support 'manifest' mode.
```

**Error** (JSON mode):
```json
{}
```

Exit code: **1**

## Why These Limitations?

Per spec edge cases:
- **Tags** require OCI registry API (no local equivalent)
- **Dependencies** require fetching the feature (could support local, but not in current spec)
- **Verbose** combines multiple modes including tags

Local features are meant for:
- Development and testing
- Pre-publish validation
- Manifest inspection

For full info operations, publish to a registry first.

## Success Criteria

- ✅ Manifest mode works for local features
- ✅ `canonicalId` is null in JSON output
- ✅ Other modes fail with clear error messages
- ✅ Error messages are user-friendly
- ✅ JSON errors produce `{}` + exit 1

## Testing Script

```bash
#!/bin/bash
# test-local-limitations.sh - Verify local feature behavior

cd examples/features-info/local-feature-only-manifest

echo "Testing manifest mode (should succeed)..."
OUTPUT=$(deacon features info manifest ./local-feature --output-format json)
if echo "$OUTPUT" | jq -e '.canonicalId == null' > /dev/null; then
  echo "  ✓ Manifest mode works with null canonicalId"
else
  echo "  ✗ Unexpected output"
  exit 1
fi

echo "Testing tags mode (should fail)..."
OUTPUT=$(deacon features info tags ./local-feature --output-format json 2>/dev/null)
if [ "$OUTPUT" = "{}" ] && [ $? -eq 1 ]; then
  echo "  ✓ Tags mode correctly rejected"
else
  echo "  ✗ Unexpected behavior"
  exit 1
fi

echo "All local feature limitation tests passed!"
```

## Files

- `local-feature/devcontainer-feature.json` - Feature metadata
- `local-feature/install.sh` - Installation script (not used by info command)

## Related Examples

- [manifest-local-feature](../manifest-local-feature/) - Full local manifest example
- [error-handling-invalid-ref](../error-handling-invalid-ref/) - Other error cases
