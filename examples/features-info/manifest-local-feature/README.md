# Manifest from Local Feature

**User Story**: US1 - Inspect manifest and canonical ID  
**Priority**: P1  
**Format**: Text (default)

## Description

Demonstrates reading feature metadata from a local directory. This is useful during feature development before publishing to a registry.

## Use Case

- Feature authors inspecting metadata during development
- Validating feature configuration before publishing
- Local testing without registry access

## Prerequisites

None - works entirely offline

## Running

```bash
cd examples/features-info/manifest-local-feature
deacon features info manifest ./sample-feature
```

## Expected Output

Two Unicode-boxed sections with local feature indication:

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Manifest                                                                  ║
╚═══════════════════════════════════════════════════════════════════════════╝
{
  "id": "sample-local-feature",
  "version": "1.0.0",
  "name": "Sample Local Feature",
  "description": "A local feature for demonstrating manifest inspection...",
  "options": {...},
  "installsAfter": [...]
}

╔═══════════════════════════════════════════════════════════════════════════╗
║ Canonical Identifier                                                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
(local feature)
```

## JSON Output

```bash
deacon features info manifest ./sample-feature --output-format json
```

Expected JSON structure:
```json
{
  "manifest": {
    "id": "sample-local-feature",
    "version": "1.0.0",
    ...
  },
  "canonicalId": null
}
```

**Note**: Local features always have `"canonicalId": null` in JSON mode since no digest is available.

## Success Criteria

- ✅ Command completes with exit code 0
- ✅ Manifest displays content from `devcontainer-feature.json`
- ✅ Canonical Identifier shows "(local feature)" in text mode
- ✅ JSON mode includes `"canonicalId": null`
- ✅ Works without network access

## Files

- `sample-feature/devcontainer-feature.json` - Feature metadata
- `sample-feature/install.sh` - Feature installation script (not used by info command)

## Related Examples

- [manifest-public-registry](../manifest-public-registry/) - Manifest from registry
- [manifest-json-output](../manifest-json-output/) - JSON format output
