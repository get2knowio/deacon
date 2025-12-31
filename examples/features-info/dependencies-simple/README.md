# Simple Dependency Graph

**User Story**: US3 - Visualize dependency graph  
**Priority**: P2  
**Format**: Text (only)

## Description

Demonstrates visualizing feature dependencies as a Mermaid graph. Shows both `dependsOn` (required dependencies) and `installsAfter` (ordering constraints) relationships.

## Use Case

- Feature authors understanding dependency relationships
- Debugging installation order issues
- Documenting feature requirements
- Planning feature composition

## Prerequisites

None - works with local features offline

## Running

```bash
cd examples/features-info/dependencies-simple
deacon features info dependencies ./my-feature
```

## Expected Output

A Unicode-boxed section with Mermaid graph syntax:

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Dependency Tree (Render with https://mermaid.live/)                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
graph TD
    my-feature["my-feature<br/>v1.0.0"]
    common-utils["ghcr.io/devcontainers/features/common-utils<br/>latest"]
    git["ghcr.io/devcontainers/features/git<br/>1"]
    
    my-feature -->|depends on| common-utils
    my-feature -->|depends on| git
    my-feature -.->|installs after| common-utils
```

## Understanding the Graph

- **Solid arrows** (`-->`) represent `dependsOn` relationships (hard dependencies)
- **Dotted arrows** (`-.->`) represent `installsAfter` relationships (ordering hints)
- Node labels show feature ID and version

## Rendering the Graph

Copy the Mermaid syntax and paste it into:
- https://mermaid.live/ for instant visualization
- Markdown files that support Mermaid
- Documentation tools with Mermaid support

## JSON Mode Not Supported

Per spec (FR-005), dependencies mode only outputs text:

```bash
deacon features info dependencies ./my-feature --output-format json
# Error: dependencies mode only supports text output
# Outputs: {}
# Exit code: 1
```

This is intentional - the Mermaid graph is designed for human visualization, not machine parsing.

## Success Criteria

- ✅ Command completes with exit code 0 (text mode)
- ✅ Output is valid Mermaid syntax
- ✅ Graph renders correctly on mermaid.live
- ✅ Both `dependsOn` and `installsAfter` shown
- ✅ JSON mode returns error and `{}`

## Files

- `my-feature/devcontainer-feature.json` - Feature with dependencies
- `my-feature/install.sh` - Installation script (not used by info command)

## Related Examples

- [dependencies-complex](../dependencies-complex/) - More complex dependency graph
- [verbose-text-output](../verbose-text-output/) - Includes dependencies in verbose mode
