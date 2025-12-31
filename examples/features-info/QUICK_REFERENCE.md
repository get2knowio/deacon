# Features Info - Quick Reference

## Commands Overview

| Mode | Description | Text Output | JSON Output | Local Feature Support |
|------|-------------|-------------|-------------|----------------------|
| `manifest` | OCI manifest + canonical ID | Boxed sections | `{manifest, canonicalId}` | ✅ Yes (canonicalId: null) |
| `tags` | Published version tags | Boxed list | `{publishedTags: [...]}` | ❌ No |
| `dependencies` | Dependency graph (Mermaid) | Boxed graph | ❌ Error + `{}` | ❌ No |
| `verbose` | All of the above | 3 boxed sections | manifest + tags only | ❌ No |

## Flag Reference

```bash
deacon features info <MODE> <FEATURE_REF> [FLAGS]
```

### Required Arguments
- `<MODE>` - One of: `manifest`, `tags`, `dependencies`, `verbose`
- `<FEATURE_REF>` - Feature reference:
  - Registry: `ghcr.io/namespace/name:tag`
  - Local: `./path/to/feature`

### Optional Flags
- `--output-format <text|json>` - Output format (default: text)
- `--log-level <info|debug|trace>` - Logging level (default: info)

## Common Patterns

### Check Feature Digest
```bash
deacon features info manifest ghcr.io/devcontainers/features/node:1 \
  --output-format json | jq -r '.canonicalId'
```

### Find Latest Version
```bash
deacon features info tags ghcr.io/devcontainers/features/node \
  --output-format json | jq -r '.publishedTags[]' | sort -V | tail -1
```

### Visualize Dependencies
```bash
deacon features info dependencies ./my-feature | pbcopy
# Paste into https://mermaid.live/
```

### Complete Feature Info
```bash
deacon features info verbose ghcr.io/devcontainers/features/node:1 \
  --output-format json > feature-info.json
```

## Exit Codes

- **0** - Success
- **1** - Error (outputs `{}` in JSON mode)

## Error Handling

### Text Mode
Human-readable error messages:
```
Error: Failed to fetch manifest: timeout after 10s
```

### JSON Mode
Always outputs `{}` on error:
```json
{}
```

### Verbose Mode Partial Failures (JSON only)
Includes partial data + errors object:
```json
{
  "manifest": {...},
  "canonicalId": "...",
  "errors": {
    "tags": "Failed to list tags: timeout after 10s"
  }
}
```
Exit code: **1**

## Performance Characteristics

| Operation | Typical Duration | Timeout | Limits |
|-----------|-----------------|---------|--------|
| Manifest fetch | 1-2s | 10s per request | - |
| Tag listing | 2-3s | 10s per request | 10 pages / 1000 tags max |
| Dependencies | <100ms | - | Local only |
| Verbose | Sum of above | 10s per sub-request | - |

## Example Directory Structure

```
examples/features-info/
├── README.md                          # Overview
├── test-all-examples.sh              # Test runner
├── QUICK_REFERENCE.md                # This file
├── manifest-public-registry/         # US1: Registry manifest
├── manifest-local-feature/           # US1: Local manifest
├── manifest-json-output/             # US1: JSON format
├── tags-public-feature/              # US2: Tag listing
├── tags-json-output/                 # US2: JSON tags
├── dependencies-simple/              # US3: Simple graph
├── dependencies-complex/             # US3: Complex graph
├── verbose-text-output/              # US4: Text verbose
├── verbose-json-output/              # US4: JSON verbose
├── error-handling-invalid-ref/       # Edge: Invalid refs
├── error-handling-network-failure/   # Edge: Timeouts
└── local-feature-only-manifest/      # Edge: Local limits
```

Each directory contains:
- `README.md` - Full documentation
- Required files (feature metadata, install scripts)
- Example commands
- Expected output
- Success criteria

## Running Examples

### All Examples
```bash
cd examples/features-info
export DEACON_NETWORK_TESTS=1  # Enable network tests
bash test-all-examples.sh
```

### Single Example
```bash
cd examples/features-info/manifest-local-feature
deacon features info manifest ./sample-feature
```

### Network Tests Only
```bash
export DEACON_NETWORK_TESTS=1
cd examples/features-info
bash test-all-examples.sh | grep "requires network" -B1
```

## Troubleshooting

### No output or hangs
- Check timeout settings (10s default)
- Enable debug logging: `--log-level debug`
- Verify network connectivity to registry

### JSON validation fails
- Ensure only JSON on stdout: `deacon ... --output-format json 2>/dev/null`
- Check exit code: `echo $?`
- Validate with jq: `deacon ... --output-format json | jq .`

### Local feature errors
- Verify `devcontainer-feature.json` exists
- Check file permissions
- Only `manifest` mode supported for local features

### Registry authentication
- Check Docker config: `cat ~/.docker/config.json`
- Set credentials: `docker login ghcr.io`
- Use debug logs: `RUST_LOG=debug deacon ...`

## Related Documentation

- Spec: `docs/subcommand-specs/features-info/SPEC.md`
- Implementation tasks: `specs/004-close-features-info-gap/tasks.md`
- Data contracts: `specs/004-close-features-info-gap/contracts/`
