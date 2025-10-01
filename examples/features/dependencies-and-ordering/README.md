# Dependencies and Ordering Example

## What This Demonstrates

This example shows how the DevContainer feature system handles:
- **Feature dependencies** via `dependsOn` field
- **Installation ordering** via `installsAfter` field  
- **Dependency graph resolution** and topological sorting
- **Cycle detection** to prevent circular dependencies

## Feature Relationships

This example includes three local features with the following dependency structure:

```
feature-a (independent)
feature-b (independent)
feature-c (depends on: feature-a, feature-b; installs after: feature-a)
```

### Feature A
- No dependencies
- Can be installed at any time
- Located in `./feature-a/`

### Feature B  
- No dependencies
- Can be installed at any time
- Located in `./feature-b/`

### Feature C
- **Depends on**: feature-a AND feature-b (via `dependsOn`)
- **Installs after**: feature-a (via `installsAfter`)
- Must be installed AFTER both A and B are installed
- Located in `./feature-c/`

## Expected Installation Order

The dependency resolver will produce an installation plan that respects both `dependsOn` and `installsAfter` constraints. Valid installation orders include:

1. **feature-a → feature-b → feature-c** (lexicographic order for independents)
2. **feature-b → feature-a → feature-c** (alternative valid order)

Both orders are correct because:
- feature-a and feature-b have no dependencies (can run in any order or parallel)
- feature-c must run AFTER both A and B complete (hard dependency)

## DevContainer Specification References

- **[Feature Dependencies](https://containers.dev/implementors/spec/#dependson)**: The `dependsOn` field creates hard dependencies
- **[Installation Ordering](https://containers.dev/implementors/spec/#installsafter)**: The `installsAfter` field provides ordering hints
- **[Dependency Resolution](https://containers.dev/implementors/spec/#feature-resolution)**: How tools resolve and order features

## Commands

View the configuration with resolved features:
```sh
deacon read-configuration --config devcontainer.json
```

Plan feature installation order (without executing):
```sh
deacon features plan --config devcontainer.json --json
```

The plan output will show the dependency graph and installation order respecting all constraints.

## Cycle Detection

The dependency resolver includes cycle detection. If you create a circular dependency (e.g., A depends on B, B depends on C, C depends on A), the resolver will detect the cycle and fail with a clear error message showing the cycle path.

To test cycle detection, you could temporarily modify the features to create a cycle:
- Add `"dependsOn": {"feature-c": true}` to feature-a/devcontainer-feature.json
- This creates: A → C → A (cycle)
- The resolver will detect and report: "Dependency cycle detected: feature-a -> feature-c -> feature-a"

## Why This Matters

Correct dependency resolution ensures:
- **Reliability**: Features install in the right order every time
- **Safety**: Cycles are detected before any installation begins
- **Predictability**: Installation plans are deterministic given the same inputs
- **Flexibility**: Independent features can be optimized (e.g., parallel installation)

## Offline Operation

This example is fully offline - all features are local (no registry required). Copy the entire directory and run commands without network access.
