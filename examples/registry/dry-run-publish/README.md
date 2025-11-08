# OCI/Registry: Dry-Run Publish Example

## What This Demonstrates

This example showcases **offline-friendly dry-run publishing** for DevContainer features and templates to OCI-compatible registries. Dry-run mode enables you to:

- **Validate artifacts** before actual publication
- **Preview publish operations** without network access
- **Test distribution workflows** in CI/CD environments
- **Verify packaging** and metadata before pushing to registries
- **Plan releases** and review what will be published

## Why This Matters

Dry-run publish is essential for:

- **Safe rehearsals**: Test publish workflows without modifying remote registries
- **CI/CD integration**: Validate artifacts in build pipelines before deployment
- **Offline development**: Work on distribution workflows without network connectivity
- **Security reviews**: Inspect what will be published before exposing to registries
- **Cost control**: Avoid unnecessary registry bandwidth and storage during testing

Real-world applications include:

- Pre-release validation of feature and template artifacts
- Automated testing of distribution workflows in CI/CD
- Local development and testing without registry credentials
- Security scanning and compliance checks before publication
- Educational demonstrations and training materials

## DevContainer Specification References

This example demonstrates distribution workflows from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Feature Distribution](https://containers.dev/implementors/spec/#distributing-features)**: OCI registry publication of features
- **[Template Distribution](https://containers.dev/implementors/spec/#distributing-templates)**: OCI registry publication of templates
- **[OCI Artifacts](https://containers.dev/implementors/spec/#oci-artifacts)**: Using OCI registries for feature and template storage
- **[Registry References](https://containers.dev/implementors/spec/#registry-reference)**: Format and resolution of OCI registry references

## CLI Specification References

See `docs/subcommand-specs/*/SPEC.md` sections:

- **Feature Distribution**: Feature publication and OCI integration
- **Template Distribution**: Template publication and OCI integration
- **Distribution Workflows**: Dry-run and publish operations

## Contents

This directory contains symlinks to existing examples to avoid duplication:

- `feature/` → `../../feature-management/feature-with-options/`
- `template/` → `../../template-management/template-with-options/`

Both artifacts demonstrate comprehensive metadata, options, and configuration suitable for registry distribution.

## Feature Dry-Run Publish

### Basic Dry-Run

Preview what would be published to a registry:

```sh
cd feature
deacon features publish . \
  --registry ghcr.io/example/my-feature \
  --dry-run \
  --json
```

### Expected Output Fields

```json
{
  "features": [
    {
      "featureId": "example/my-feature",
      "version": "1.2.3",
      "digest": "sha256:dryrun0000000000000000000000000000000000000000000000000000000000",
      "publishedTags": ["1", "1.2", "1.2.3", "latest"],
      "skippedTags": [],
      "movedLatest": true,
      "registry": "ghcr.io",
      "namespace": "example"
    }
  ],
  "summary": {
    "features": 1,
    "publishedTags": 4,
    "skippedTags": 0
  }
}
```

**Output Fields:**
- `features`: Array of feature publish results
  - `featureId`: Feature identifier
  - `version`: Semantic version
  - `digest`: SHA256 digest (dry-run prefixed)
  - `publishedTags`: Tags that would be published
  - `skippedTags`: Tags that would be skipped
  - `movedLatest`: Whether latest tag was moved
  - `registry`: Target registry
  - `namespace`: Target namespace
- `summary`: Aggregate statistics
  - `features`: Number of features processed
  - `publishedTags`: Total tags to publish
  - `skippedTags`: Total tags to skip

### Extract Specific Fields with jq

```sh
# Get the feature ID
cd feature
deacon features publish . \
  --registry ghcr.io \
  --namespace example \
  --dry-run \
  --json 2>/dev/null | jq -r '.features[0].featureId'

# Check published tags
cd feature
deacon features publish . \
  --registry ghcr.io \
  --namespace example \
  --dry-run \
  --json 2>/dev/null | jq -r '.features[0].publishedTags[]'

# Get summary statistics
cd feature
deacon features publish . \
  --registry ghcr.io \
  --namespace example \
  --dry-run \
  --json 2>/dev/null | jq '.summary'
```

### Multiple Registry Targets

Test publishing to different registry paths:

```sh
cd feature

# GitHub Container Registry
deacon features publish . \
  --registry ghcr.io \
  --namespace myorg/my-feature \
  --dry-run --json 2>/dev/null | jq -r '.features[0].registry'

# Docker Hub
deacon features publish . \
  --registry docker.io \
  --namespace myuser/my-feature \
  --dry-run --json 2>/dev/null | jq -r '.features[0].registry'

# Private registry
deacon features publish . \
  --registry registry.example.com \
  --namespace features/my-feature \
  --dry-run --json 2>/dev/null | jq -r '.features[0].registry'
```

## Template Dry-Run Publish

**Note**: Template publishing is not yet implemented in this feature. The template directory is included for future reference when template publishing is added.

The template publishing will follow a similar JSON output structure but with template-specific fields.

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Validate Feature Distribution

on: [pull_request]

jobs:
  validate-publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Deacon
        run: |
          # Install deacon (placeholder - adjust for actual installation)
          cargo install --path crates/deacon
      
      - name: Dry-run publish feature
        run: |
          cd examples/registry/dry-run-publish/feature
          deacon features publish . \
            --registry ghcr.io \
            --namespace ${{ github.repository }} \
            --dry-run \
            --json > publish-result.json
          
          # Validate output structure
          jq -e '.features | length == 1' publish-result.json
          jq -e '.summary.features == 1' publish-result.json
          jq -e '.features[0].publishedTags | length > 0' publish-result.json
      
      - name: Dry-run publish template
        run: |
          # Template publishing not yet implemented
          echo "Template publishing will be added in a future feature"
```

## Comparison: Dry-Run vs. Actual Publish

### Dry-Run Mode (No Network Required)
```sh
cd feature
deacon features publish . \
  --registry ghcr.io \
  --namespace example/my-feature \
  --dry-run \
  --json
# ✓ No authentication required
# ✓ No network access needed
# ✓ Returns structured JSON with feature details
# ✓ Validates local artifact structure
```

### Actual Publish (Network & Authentication Required)
```sh
cd feature
# This would require:
# - Valid registry credentials
# - Network connectivity
# - Push permissions to the registry
deacon features publish . \
  --registry ghcr.io \
  --namespace example/my-feature \
  --json
# ✗ Requires authentication
# ✗ Needs network access
# ✓ Returns real digest from registry
# ✓ Actually publishes the artifact
```

## Advanced jq Patterns

### Validate Multiple Artifacts

```sh
# Check both feature and template in one command
(cd feature && deacon features publish . \
  --registry ghcr.io/example/my-feature \
  --dry-run --json 2>/dev/null) \
| jq '{feature: .}' > /tmp/results.json

(cd template && deacon templates publish . \
  --registry ghcr.io/example/my-template \
  --dry-run 2>/dev/null) \
| jq '{template: .}' >> /tmp/results.json

jq -s '.[0] * .[1]' /tmp/results.json
```

### Create Publish Report

```sh
# Generate a report of planned publishes
echo "# Publish Report" > report.md
echo "" >> report.md

echo "## Feature: feature-with-options" >> report.md
cd feature
deacon features publish . \
  --registry ghcr.io/example/my-feature \
  --dry-run --json 2>/dev/null \
| jq -r '"- Registry: " + (.message | sub("Dry run completed - would publish to "; ""))' \
>> ../report.md
echo "" >> ../report.md

echo "## Template: template-with-options" >> report.md
cd ../template
deacon templates publish . \
  --registry ghcr.io/example/my-template \
  --dry-run 2>/dev/null \
| jq -r '"- Registry: " + (.message | sub("Dry run completed - would publish to "; ""))' \
>> ../report.md

cd ..
cat report.md
rm report.md
```

## Verification Checklist

Before actual publication, verify:

- [ ] Dry-run completes successfully (`"status": "success"`)
- [ ] Metadata is valid (check `devcontainer-feature.json` or `devcontainer-template.json`)
- [ ] Required files are present (features need install scripts, templates need all listed files)
- [ ] Registry path is correct and accessible (when doing actual publish)
- [ ] Authentication credentials are available (for actual publish)
- [ ] Version numbers follow semantic versioning
- [ ] Documentation is up to date

## Notes

- **No Network Required**: Dry-run mode works completely offline
- **Placeholder Digests**: Dry-run returns `sha256:dryrun...` prefixed digests
- **Validation Only**: Dry-run validates artifact structure but doesn't contact registries
- **CI-Friendly**: Perfect for automated testing in CI/CD pipelines
- **Safe Testing**: Experiment with registry paths and options without side effects

## Next Steps

After validating with dry-run:

1. **Set up authentication**: Configure registry credentials
2. **Remove --dry-run flag**: Perform actual publish
3. **Verify publication**: Pull the artifact to confirm it's accessible
4. **Update documentation**: Document the published artifact location
5. **Test integration**: Verify the artifact works in actual DevContainer configurations

## Related Examples

- `examples/feature-management/feature-with-options/`: Source feature with comprehensive options
- `examples/template-management/template-with-options/`: Source template with comprehensive options
- `examples/features/`: Feature installation and resolution examples
