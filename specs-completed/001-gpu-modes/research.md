# Phase 0 Research: GPU Mode Handling for Up

## Tasks Dispatched
- Research GPU capability detection approach for `detect` mode in the `up` workflow.
- Find best practices for propagating GPU requests across Docker run/build and Compose operations.
- Find best practices for user-facing warnings/output when GPUs are absent under auto-detect behavior.

## Findings

### Decision: GPU detection via Docker runtime introspection with graceful fallback
- **Rationale**: Checking Docker runtime info (e.g., presence of GPU-capable runtimes such as `nvidia`) avoids dependency on vendor-specific binaries and matches container lifecycle context. If the query fails or no GPU runtime is present, we proceed without GPUs and warn once, aligning with the spec’s “detect and warn” intent.
- **Alternatives considered**:  
  - Shelling to `nvidia-smi` or vendor tools (fails on systems without drivers; adds coupling).  
  - Attempting a probe container (slower; risks side effects and unnecessary pulls).  
  - Relying on environment variables alone (unreliable; misses actual runtime support).

### Decision: Propagate GPU mode consistently to Docker run/build and Compose invocations
- **Rationale**: Using the same GPU mode decision for all `up`-triggered run/build/compose calls prevents divergence between services and mirrors the spec requirement for uniform application. Compose paths should receive equivalent GPU requests (e.g., `--gpus all` when supported) to match direct run/build behavior.
- **Alternatives considered**:  
  - Only flagging `docker run` and skipping Compose/build (creates inconsistent runtime behavior; violates acceptance).  
  - Making Compose opt-in separately (adds configuration drift and conflicts with “apply uniformly” acceptance).  
  - Injecting service-level compose YAML edits instead of flagging (heavier-handed; increases mutation surface).

### Decision: Single warning on detect-without-GPU, before startup
- **Rationale**: Issuing one clear warning prior to container/build start informs users without spamming logs and keeps stdout/stderr contracts intact. It also ensures the “detect” path does not silently skip GPU usage.
- **Alternatives considered**:  
  - Warning per container/service (noisy in multi-service projects).  
  - Silently skipping GPUs (violates “warn when absent”).  
  - Aborting when GPUs are absent (violates “continue without blocking” behavior).
