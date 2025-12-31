# Parallel Installation Demo

## What This Demonstrates

This example shows how the DevContainer feature system can:
- **Install features concurrently** when they have no dependencies
- **Optimize installation time** by running independent features in parallel
- **Compute parallel execution levels** using dependency graph analysis

## Feature Setup

This example includes two independent features:

### Independent Feature 1
- No dependencies
- Simulates work with a 2-second sleep
- Located in `./independent-1/`

### Independent Feature 2
- No dependencies  
- Simulates work with a 2-second sleep
- Located in `./independent-2/`

## Expected Behavior

Since both features are independent (no `dependsOn` or `installsAfter` constraints), they can be installed in parallel:

### Sequential Installation (without parallelism)
- Total time: ~4 seconds (2 + 2)
- Features run one after another

### Parallel Installation (with parallelism)
- Total time: ~2 seconds (both run simultaneously)
- Features run concurrently

## Parallel Execution Levels

The dependency resolver computes "parallel execution levels":
- **Level 0**: Both independent-1 and independent-2 (can run in parallel)

If there were dependencies, they would be in different levels:
- **Level 0**: Features with no dependencies
- **Level 1**: Features depending only on Level 0 features
- **Level 2**: Features depending on Level 1 features
- And so on...

## DevContainer Specification References

- **[Feature Installation](https://containers.dev/implementors/spec/#feature-installation)**: How features are installed
- **[Performance Considerations](https://containers.dev/implementors/spec/#performance)**: Parallel execution strategies
- See Features Plan SPEC: ../../../docs/subcommand-specs/features-plan/SPEC.md

## Commands

View the installation plan showing parallel levels:
```sh
deacon features plan --config devcontainer.json --json
```

The output will show:
```json
{
  "order": ["independent-1", "independent-2"],
  "graph": {...},
  "levels": [
    ["independent-1", "independent-2"]
  ]
}
```

The `levels` array indicates both features are in the same level (Level 0) and can run in parallel.

## Timing Comparison (Informational)

While exact timing is not asserted in tests, you can observe the difference:

**Note**: The actual parallel execution implementation depends on the feature installation backend. This example demonstrates the *planning* phase that enables parallelism - the actual concurrent execution would be implemented by the container runtime or installation orchestrator.

The key insight: The dependency resolver identifies which features CAN run in parallel by computing execution levels. A full implementation would then execute all features in the same level concurrently.

## Why This Matters

Parallel installation provides significant benefits:
- **Faster setup times**: Especially with many independent features
- **Better resource utilization**: CPU cores used efficiently
- **Scalability**: Handles complex feature sets without linear time growth
- **Automatic optimization**: No manual tuning required

## Concurrency Control

Real implementations should respect:
- **Resource conflicts**: Don't run features that conflict (e.g., both write to same file)
- **Concurrency limits**: Bound parallelism to available resources (CPU cores, memory)
- **Error propagation**: Stop dependent features if a dependency fails

The `DEACON_MAX_FEATURE_CONCURRENCY` environment variable can control the maximum number of concurrent feature installations.

## Offline Operation

This example is fully offline - all features are local (no registry required). Copy the entire directory and run commands without network access.
