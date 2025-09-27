# Template With Options Example

## What This Demonstrates

This example showcases a comprehensive DevContainer template that demonstrates the full power of the template system for creating sophisticated project scaffolding. It includes:

- **Template options**: Multiple option types (boolean, string, enum) with defaults and validation
- **Recommended features**: Pre-configured features that work well with the template
- **File management**: Explicit listing of files included in the template
- **Platform support**: Cross-platform compatibility declarations
- **Rich metadata**: Publisher information, documentation URLs, and keywords
- **Project customization**: User-configurable options that affect generated projects

## Why This Matters

Advanced templates like this enable:
- **Enterprise project scaffolding**: Sophisticated project generators for organizations
- **Multi-language support**: Templates that adapt to different programming languages and frameworks
- **Configurable workflows**: Users can customize generated projects through well-defined options
- **Community sharing**: Publishing complex project templates to public registries
- **Standardized architectures**: Enforcing organizational patterns and best practices

Real-world applications include:
- Microservice templates with customizable language and database options
- Full-stack application templates with configurable frontend and backend technologies
- Data science project templates with environment and tool selection
- Documentation sites with theme and configuration options

## Template Files

This template includes auxiliary files that demonstrate project scaffolding:
- **`Dockerfile`**: Container configuration for the generated project
- **`src/main.py`**: Sample source code showing project structure
- **`config/app.conf`**: Configuration files for the application

## DevContainer Specification References

This example demonstrates advanced patterns from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Template Options](https://containers.dev/implementors/spec/#template-option-resolution)**: Comprehensive option types, validation, and substitution
- **[Template Files](https://containers.dev/implementors/spec/#template-file-list)**: Managing and organizing template file assets
- **[Recommended Features](https://containers.dev/implementors/spec/#template-supported-features)**: Suggesting features that complement the template
- **[Template Metadata](https://containers.dev/implementors/spec/#devcontainer-template-json-properties)**: Complete metadata specification including publisher info
- **[Template Distribution](https://containers.dev/implementors/spec/#distributing-templates)**: Platform support and distribution considerations

## Inspect
```sh
cat devcontainer-template.json | jq '.'
```
