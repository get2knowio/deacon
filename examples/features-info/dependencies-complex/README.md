# Complex Dependency Graph

**User Story**: US3 - Visualize dependency graph  
**Priority**: P2  
**Format**: Text (only)

## Description

Demonstrates visualizing a more complex feature dependency graph with multiple dependencies and ordering constraints. Useful for understanding intricate feature compositions.

## Use Case

- Understanding complex feature stacks
- Planning installation order for multiple features
- Identifying potential circular dependencies
- Documenting feature architecture

## Prerequisites

None - works with local features offline

## Running

```bash
cd examples/features-info/dependencies-complex
deacon features info dependencies ./app-feature
```

## Expected Output

A Unicode-boxed section with a more elaborate Mermaid graph:

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Dependency Tree (Render with https://mermaid.live/)                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
graph TD
    app-feature["app-feature<br/>v2.1.0"]
    common-utils["ghcr.io/devcontainers/features/common-utils<br/>2"]
    node["ghcr.io/devcontainers/features/node<br/>1"]
    docker["ghcr.io/devcontainers/features/docker-in-docker<br/>2"]
    custom-tools["./custom-tools<br/>"]
    
    app-feature -->|depends on| common-utils
    app-feature -->|depends on| node
    app-feature -->|depends on| docker
    app-feature -->|depends on| custom-tools
    app-feature -.->|installs after| common-utils
    app-feature -.->|installs after| node
    app-feature -.->|installs after| custom-tools
```

## Graph Complexity

This example demonstrates:
- **Multiple dependencies**: 4 features depend on
- **Mixed dependency types**: Registry refs and local paths
- **Ordering constraints**: 3 installsAfter relationships
- **Version specifications**: Different version requirements per dependency

## Installation Order Implications

From the graph:
1. `common-utils`, `node`, and `custom-tools` must install first
2. `app-feature` must wait for those three
3. `docker-in-docker` can install any time before `app-feature`

## Success Criteria

- ✅ All dependencies shown in graph
- ✅ Both dependency types (solid/dotted arrows) visible
- ✅ Graph renders correctly on mermaid.live
- ✅ Complex relationships are clear
- ✅ Local path dependencies included

## Files

- `app-feature/devcontainer-feature.json` - Feature with complex dependencies
- `app-feature/install.sh` - Installation script (not used by info command)

## Related Examples

- [dependencies-simple](../dependencies-simple/) - Basic dependency graph
- [verbose-text-output](../verbose-text-output/) - Verbose output with dependencies
