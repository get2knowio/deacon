# Template Apply Example

## What This Demonstrates

This example demonstrates the template application workflow, showing how to:

- **Apply templates locally**: Use `deacon templates apply` to apply a template to an output directory
- **Template option substitution**: Pass options to customize the generated project
- **Overwrite protection**: Understand how existing files are handled (skip by default)
- **Force overwrite**: Use `--force` flag to overwrite existing files
- **Dry run mode**: Preview template application without making changes
- **File interpolation**: Verify that template variables are correctly substituted in output files

## Why This Matters

Template application is crucial for:
- **Project scaffolding**: Quickly create new projects from standardized templates
- **Consistent structure**: Ensure projects follow organizational patterns
- **Customization**: Allow users to configure projects through template options
- **Safe updates**: Protect existing files from accidental overwrites
- **Preview changes**: Use dry-run to understand what will be created before committing

Real-world applications include:
- Creating new microservices from organizational templates
- Setting up standardized project structures for teams
- Generating boilerplate code with custom configurations
- Initializing projects with specific tooling and dependencies

## DevContainer Specification References

This example demonstrates patterns from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Template Application Workflow](https://containers.dev/implementors/spec/#template-application)**: How templates are applied to create projects
- **[Template Option Resolution](https://containers.dev/implementors/spec/#template-option-resolution)**: How template options are processed and substituted
- **[Template Files](https://containers.dev/implementors/spec/#template-file-list)**: File management and organization in templates

## Prerequisites

Before running these examples:
1. The Deacon CLI tool is installed and available in your PATH
2. The `template-with-options` template exists in the repository (we'll reference it)
3. No Docker or internet connection required - this is purely local file operations

## Example 1: Basic Template Application

Apply the template to a new directory with default options:

```sh
cd examples/template-management/templates-apply

# Create a temporary output directory
mkdir -p /tmp/test-project-1

# Apply the template with default options
deacon templates apply ../template-with-options --output /tmp/test-project-1

# Verify the generated files
ls -la /tmp/test-project-1/
cat /tmp/test-project-1/README.md
```

### Expected Output

The command should:
1. Copy all template files to the output directory
2. Substitute template variables using default option values
3. Report the number of files processed
4. Show which files were copied

Example log output:
```
INFO Successfully processed 4 files
INFO Copied ../template-with-options/Dockerfile -> /tmp/test-project-1/Dockerfile (with variable substitution)
INFO Copied ../template-with-options/README.md -> /tmp/test-project-1/README.md (with variable substitution)
INFO Template application completed. Files processed: 4, skipped: 0
```

## Example 2: Template Application with Custom Options

Apply the template with custom option values:

```sh
cd examples/template-management/templates-apply

# Create output directory
mkdir -p /tmp/test-project-2

# Apply template with custom options
deacon templates apply ../template-with-options \
  --output /tmp/test-project-2 \
  --option customName=my-awesome-app \
  --option debugMode=true \
  --option version=latest \
  --option enableFeature=false

# Verify substitution worked
grep -r "my-awesome-app" /tmp/test-project-2/
```

### Expected Behavior

The template options should be substituted in the output files:
- `${templateOption:customName}` → `my-awesome-app`
- `${templateOption:debugMode}` → `true`
- `${templateOption:version}` → `latest`
- `${templateOption:enableFeature}` → `false`

## Example 3: Overwrite Protection (Default Behavior)

Try to apply the template again to the same directory:

```sh
cd examples/template-management/templates-apply

# Apply template to existing directory (from Example 1)
deacon templates apply ../template-with-options --output /tmp/test-project-1
```

### Expected Behavior

The command should:
1. Detect that files already exist
2. Skip existing files (not overwrite them)
3. Report the number of files skipped

Example output:
```
INFO Successfully processed 0 files
INFO Skipped 4 existing files (use --force to overwrite)
INFO Skipped existing file: /tmp/test-project-1/Dockerfile
INFO Skipped existing file: /tmp/test-project-1/README.md
```

## Example 4: Force Overwrite

Force overwrite existing files with `--force`:

```sh
cd examples/template-management/templates-apply

# Force overwrite with different options
deacon templates apply ../template-with-options \
  --output /tmp/test-project-1 \
  --option customName=updated-project \
  --force

# Verify the file was updated
grep "updated-project" /tmp/test-project-1/README.md
```

### Expected Behavior

With `--force`:
1. Existing files are overwritten
2. New option values are substituted
3. Reports files as "Overwritten" instead of "Skipped"

Example output:
```
INFO Successfully processed 4 files
INFO Overwritten ../template-with-options/Dockerfile -> /tmp/test-project-1/Dockerfile (with variable substitution)
INFO Overwritten ../template-with-options/README.md -> /tmp/test-project-1/README.md (with variable substitution)
```

## Example 5: Dry Run Mode

Preview template application without making changes:

```sh
cd examples/template-management/templates-apply

# Dry run to see what would happen
deacon templates apply ../template-with-options \
  --output /tmp/test-project-3 \
  --option customName=preview-only \
  --dry-run

# Verify directory was NOT created
ls /tmp/test-project-3 2>&1
```

### Expected Behavior

In dry-run mode:
1. No files are created or modified
2. Shows what operations would be performed
3. Reports with "DRY RUN: Would process X files"
4. Output directory is not created

Example output:
```
INFO DRY RUN: Would process 4 files
INFO Would copy ../template-with-options/Dockerfile -> /tmp/test-project-3/Dockerfile (with variable substitution)
INFO Would copy ../template-with-options/README.md -> /tmp/test-project-3/README.md (with variable substitution)
```

## Verification Steps

### 1. Verify File Structure
```sh
# Check that all expected files were created
ls -R /tmp/test-project-1/
# Should show: Dockerfile, README.md, config/, src/
```

### 2. Verify Variable Substitution
```sh
# Check that template variables were replaced
cat /tmp/test-project-1/README.md | grep -E "customName|debugMode|version"
# Should show actual values, not ${templateOption:...} placeholders
```

### 3. Verify Option Types
```sh
# Boolean should render as "true"/"false" (not "True"/"False")
grep -i "debug.*true\|false" /tmp/test-project-1/config/app.conf

# String values should be substituted correctly
grep "customName" /tmp/test-project-1/README.md

# Enum values should match allowed values
grep -E "version.*(latest|stable|beta)" /tmp/test-project-1/README.md
```

## Testing Strategies

### Test with Invalid Options
```sh
cd examples/template-management/templates-apply

# Try invalid enum value (should fail)
deacon templates apply ../template-with-options \
  --output /tmp/test-invalid \
  --option version=invalid-version 2>&1

# Try invalid boolean value (should fail)
deacon templates apply ../template-with-options \
  --output /tmp/test-invalid \
  --option debugMode=maybe 2>&1
```

### Test Multiple Applies with Different Options
```sh
cd examples/template-management/templates-apply

# Create projects with different configurations
for name in app-dev app-staging app-prod; do
  mkdir -p /tmp/$name
  deacon templates apply ../template-with-options \
    --output /tmp/$name \
    --option customName=$name \
    --option version=$([ "$name" = "app-prod" ] && echo "stable" || echo "latest")
done

# Compare generated files
diff /tmp/app-dev/README.md /tmp/app-prod/README.md
```

## Key Observations

1. **Default Behavior is Safe**: Templates don't overwrite existing files without `--force`
2. **Dry Run for Confidence**: Use `--dry-run` to preview changes before applying
3. **Option Validation**: Invalid option values are caught and reported
4. **Variable Substitution**: Template variables are replaced throughout all files
5. **File Structure Preserved**: Directory structure from template is maintained in output
6. **Offline Operation**: Template application works entirely offline

## Common Patterns

### Creating Multiple Projects from One Template
```sh
# Use a loop to create multiple projects with variations
for env in dev staging production; do
  deacon templates apply ../template-with-options \
    --output /tmp/myapp-$env \
    --option customName=myapp-$env \
    --option version=$([ "$env" = "production" ] && echo "stable" || echo "latest")
done
```

### Conditional Overwrite Based on Changes
```sh
# Apply template, check if files changed, then decide to keep or revert
deacon templates apply ../template-with-options \
  --output /tmp/existing-project \
  --force

# If not satisfied, restore from backup or git
```

## Notes

- Template application is a local file operation - no network or Docker required
- Template options are validated against the schema in `devcontainer-template.json`
- Variable substitution happens at application time, not at template creation time
- Use `--dry-run` frequently to understand what will happen before committing to changes

## Spec References

- subcommand-specs/*/SPEC.md: Template Application Workflow
- subcommand-specs/*/SPEC.md: Template System
- DevContainer Spec: Template Option Resolution
- DevContainer Spec: Template Files
