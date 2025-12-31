# Template Metadata and Documentation Example

## What This Demonstrates

This example demonstrates template introspection and documentation generation, showing how to:

- **Inspect template metadata**: Use `deacon templates metadata` to view template configuration
- **View template options**: Examine available options, types, defaults, and descriptions
- **Generate documentation**: Use `deacon templates generate-docs` to create markdown documentation
- **Verify documentation structure**: Ensure generated docs contain all expected sections
- **Understand template capabilities**: Learn about recommended features and template properties

## Why This Matters

Template metadata inspection and documentation generation are essential for:
- **Template discovery**: Understanding what a template provides before using it
- **Option configuration**: Learning which options are available and how to use them
- **Template publishing**: Automatically generating user-facing documentation
- **Quality assurance**: Verifying templates have complete and accurate metadata
- **Team collaboration**: Sharing template capabilities with other developers

Real-world applications include:
- Browsing available templates in a registry
- Generating README files for published templates
- Creating template catalogs for organizations
- Validating template structure before publishing
- Documenting template options for users

## DevContainer Specification References

This example demonstrates patterns from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Template Metadata](https://containers.dev/implementors/spec/#devcontainer-template-json-properties)**: Complete metadata specification
- **[Template Options](https://containers.dev/implementors/spec/#template-option-resolution)**: Option types and validation
- **[Template Distribution](https://containers.dev/implementors/spec/#distributing-templates)**: Publishing and documentation requirements

## Prerequisites

Before running these examples:
1. The Deacon CLI tool is installed and available in your PATH
2. The `template-with-options` template exists in the repository
3. No Docker or internet connection required - purely local operations

## Example 1: View Template Metadata

Inspect the complete metadata for a template:

```sh
cd examples/template-management/metadata-and-docs

# View template metadata
deacon templates metadata ../template-with-options
```

### Expected Output

The command outputs JSON showing the template's complete metadata:

```json
{
  "id": "template-with-options",
  "name": "Template with Options",
  "options": {
    "customName": {
      "default": "my-project",
      "description": "Custom project name",
      "enum": null,
      "proposals": null,
      "type": "string"
    },
    "debugMode": {
      "default": false,
      "description": "Enable debug mode",
      "type": "boolean"
    },
    "enableFeature": {
      "default": true,
      "description": "Enable the experimental feature",
      "type": "boolean"
    },
    "version": {
      "default": "stable",
      "description": "Version to install",
      "enum": [
        "latest",
        "stable",
        "beta"
      ],
      "proposals": null,
      "type": "string"
    }
  },
  "recommendedFeatures": {
    "ghcr.io/devcontainers/features/docker-in-docker:2": {
      "version": "latest"
    },
    "ghcr.io/devcontainers/features/git:1": {}
  }
}
```

### Key Sections to Verify

1. **Template Identity**: `id` and `name` fields
2. **Options Definition**: Each option with type, default, and description
3. **Enum Values**: For options with restricted values (like `version`)
4. **Recommended Features**: Features that work well with this template

## Example 2: Extract Specific Option Information

Use `jq` to query specific parts of the metadata:

```sh
cd examples/template-management/metadata-and-docs

# View only option names
deacon templates metadata ../template-with-options | jq '.options | keys'

# View only boolean options
deacon templates metadata ../template-with-options | jq '.options | to_entries | map(select(.value.type == "boolean")) | from_entries'

# View options with enum values
deacon templates metadata ../template-with-options | jq '.options | to_entries | map(select(.value.enum != null))'

# View recommended features
deacon templates metadata ../template-with-options | jq '.recommendedFeatures'
```

### Expected Results

**Option names:**
```json
[
  "customName",
  "debugMode",
  "enableFeature",
  "version"
]
```

**Boolean options:**
```json
{
  "debugMode": {
    "default": false,
    "description": "Enable debug mode",
    "type": "boolean"
  },
  "enableFeature": {
    "default": true,
    "description": "Enable the experimental feature",
    "type": "boolean"
  }
}
```

## Example 3: Generate Template Documentation

Generate markdown documentation for a template:

```sh
cd examples/template-management/metadata-and-docs

# Create output directory
mkdir -p /tmp/template-docs

# Generate documentation
deacon templates generate-docs ../template-with-options --output /tmp/template-docs

# View generated documentation
cat /tmp/template-docs/README-template.md
```

### Expected Documentation Structure

The generated `README-template.md` should contain:

1. **Title**: Template name as H1 heading
2. **Description**: Template description paragraph
3. **Options Section**: H2 heading "Options"
4. **Option Details**: Each option as H3 with:
   - Description
   - Type
   - Default value
   - Enum values (if applicable)
5. **Usage Section**: Example JSON configuration

Example generated content:
```markdown
# Template with Options

A DevContainer template with various option types

## Options

### customName
Custom project name
- Type: `string`
- Default: `my-project`

### debugMode
Enable debug mode
- Type: `boolean`
- Default: `false`

### enableFeature
Enable the experimental feature
- Type: `boolean`
- Default: `true`

### version
Version to install
- Type: `string`
- Default: `stable`
- Allowed values: `latest`, `stable`, `beta`

## Usage

```json
{
  "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
  "features": {
    "template-with-options": {}
  }
}
```
```

## Example 4: Verify Documentation Completeness

Check that all expected sections are present:

```sh
cd examples/template-management/metadata-and-docs

# Generate docs
deacon templates generate-docs ../template-with-options --output /tmp/template-docs

# Count sections
grep -c "^## " /tmp/template-docs/README-template.md
# Should be 2 (Options, Usage)

# Verify all options are documented
grep -c "^### " /tmp/template-docs/README-template.md
# Should be 4 (customName, debugMode, enableFeature, version)

# Check for required keywords
grep -q "Type:" /tmp/template-docs/README-template.md && echo "✓ Type information present"
grep -q "Default:" /tmp/template-docs/README-template.md && echo "✓ Default values present"
grep -q "Allowed values:" /tmp/template-docs/README-template.md && echo "✓ Enum values present"
```

### Expected Output
```
2
4
✓ Type information present
✓ Default values present
✓ Enum values present
```

## Example 5: Compare Multiple Templates

Compare metadata from different templates:

```sh
cd examples/template-management/metadata-and-docs

# Generate metadata for multiple templates
deacon templates metadata ../template-with-options > /tmp/with-options.json
deacon templates metadata ../minimal-template > /tmp/minimal.json

# Compare option counts
echo "With-options has $(jq '.options | length' /tmp/with-options.json) options"
echo "Minimal has $(jq '.options | length // 0' /tmp/minimal.json) options"

# Compare recommended features
echo "With-options recommends $(jq '.recommendedFeatures | length' /tmp/with-options.json) features"
echo "Minimal recommends $(jq '.recommendedFeatures | length // 0' /tmp/minimal.json) features"
```

### Expected Results
```
With-options has 4 options
Minimal has 0 options
With-options recommends 2 features
Minimal recommends 0 features
```

## Verification Steps

### 1. Verify Metadata Command Output Format
```sh
# Ensure output is valid JSON
deacon templates metadata ../template-with-options | jq '.' > /dev/null && echo "✓ Valid JSON"

# Check for required fields
deacon templates metadata ../template-with-options | jq 'has("id", "name", "options")' 
# Should output: true
```

### 2. Verify Documentation File Creation
```sh
# Check that README was created
test -f /tmp/template-docs/README-template.md && echo "✓ Documentation file exists"

# Check file is not empty
test -s /tmp/template-docs/README-template.md && echo "✓ Documentation has content"

# Check for markdown headers
grep -q "^#" /tmp/template-docs/README-template.md && echo "✓ Markdown headers present"
```

### 3. Verify Option Documentation Consistency
```sh
# Extract option names from metadata
METADATA_OPTIONS=$(deacon templates metadata ../template-with-options | jq -r '.options | keys[]' | sort)

# Extract option names from generated docs
DOCS_OPTIONS=$(grep "^### " /tmp/template-docs/README-template.md | sed 's/### //' | sort)

# Compare (should match)
if [ "$METADATA_OPTIONS" = "$DOCS_OPTIONS" ]; then
  echo "✓ All options documented"
else
  echo "✗ Option mismatch"
  echo "In metadata: $METADATA_OPTIONS"
  echo "In docs: $DOCS_OPTIONS"
fi
```

## Testing Strategies

### Test with Minimal Template

```sh
cd examples/template-management/metadata-and-docs

# Test with template that has no options
deacon templates metadata ../minimal-template

# Generate docs for minimal template
deacon templates generate-docs ../minimal-template --output /tmp/minimal-docs

# Verify docs are generated even without options
cat /tmp/minimal-docs/README-template.md
```

### Test Metadata JSON Parsing

```sh
cd examples/template-management/metadata-and-docs

# Parse metadata into shell variables
TEMPLATE_ID=$(deacon templates metadata ../template-with-options | jq -r '.id')
TEMPLATE_NAME=$(deacon templates metadata ../template-with-options | jq -r '.name')
OPTION_COUNT=$(deacon templates metadata ../template-with-options | jq '.options | length')

echo "Template: $TEMPLATE_NAME ($TEMPLATE_ID)"
echo "Options: $OPTION_COUNT"
```

### Test Documentation Determinism

```sh
cd examples/template-management/metadata-and-docs

# Generate docs twice
deacon templates generate-docs ../template-with-options --output /tmp/docs1
deacon templates generate-docs ../template-with-options --output /tmp/docs2

# Compare outputs (should be identical)
diff /tmp/docs1/README-template.md /tmp/docs2/README-template.md && echo "✓ Documentation is deterministic"
```

## Key Observations

1. **Metadata is Always JSON**: Output format is consistent and parseable
2. **Options are Sorted**: Options appear in alphabetical order for consistency
3. **Complete Information**: All option properties (type, default, enum) are included
4. **Generated Docs are Markdown**: Standard markdown format with predictable structure
5. **Offline Operation**: No network or Docker required for metadata or docs commands

## Common Patterns

### Extract Option Defaults for Automation
```sh
# Get all default values as shell variables
eval $(deacon templates metadata ../template-with-options | \
  jq -r '.options | to_entries | .[] | "TEMPLATE_\(.key | ascii_upcase)=\(.value.default)"')

echo "Default customName: $TEMPLATE_CUSTOMNAME"
echo "Default debugMode: $TEMPLATE_DEBUGMODE"
```

### Generate Documentation for All Templates
```sh
# Find all templates and generate docs
for template_dir in examples/template-management/*/; do
  if [ -f "$template_dir/devcontainer-template.json" ]; then
    template_name=$(basename "$template_dir")
    echo "Generating docs for $template_name..."
    deacon templates generate-docs "$template_dir" --output "/tmp/docs-$template_name"
  fi
done
```

### Validate Template Before Publishing
```sh
# Check that template has required metadata fields
METADATA=$(deacon templates metadata ../template-with-options)

# Validate required fields
echo "$METADATA" | jq -e '.id' > /dev/null && echo "✓ Has ID"
echo "$METADATA" | jq -e '.name' > /dev/null && echo "✓ Has name"
echo "$METADATA" | jq -e '.options' > /dev/null && echo "✓ Has options"

# Check that each option has required fields
echo "$METADATA" | jq -e '.options | .[] | .type' > /dev/null && echo "✓ All options have type"
echo "$METADATA" | jq -e '.options | .[] | .description' > /dev/null && echo "✓ All options have description"
```

## Notes

- Metadata inspection works on local template directories only
- Generated documentation follows a standard format across all templates
- Both commands are offline operations requiring no network or Docker
- Metadata output is deterministic and suitable for automation
- Documentation generation creates a single `README-template.md` file

## Spec References

- subcommand-specs/*/SPEC.md: Template System
- subcommand-specs/*/SPEC.md: Template Metadata
- DevContainer Spec: Template Properties
- DevContainer Spec: Template Options
