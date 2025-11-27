# Data Model: GPU Mode Handling for Up

## Entities

### GPUMode
- **Description**: Enum representing how `up` handles GPU requests.
- **Values**:
  - `all`: Always request GPU resources.
  - `detect`: Probe host GPU capability; request if available, else warn and skip.
  - `none`: Never request GPU resources (default when unspecified).
- **Constraints**: Only these three values are valid; default resolves to `none` if user does not set a mode.

### HostGpuCapability
- **Description**: Result of host GPU probing for `detect` mode.
- **Fields**:
  - `available` (bool): Whether GPU-capable runtime is detected.
  - `runtime_name` (string, optional): Name of detected GPU runtime (e.g., `nvidia`); omitted if none.
  - `probe_error` (string, optional): Warning detail if detection fails; mutually exclusive with `available == true`.
- **Constraints**: Detection is best-effort; probe failures still allow execution with a warning.

### UpGpuApplication
- **Description**: Captures how the selected GPU mode is applied during a single `up` invocation.
- **Fields**:
  - `selected_mode` (GPUMode): User-specified or defaulted mode.
  - `applies_to_run` (bool): GPU request applied to docker run paths.
  - `applies_to_build` (bool): GPU request applied to docker build paths when supported.
  - `applies_to_compose` (bool): GPU request applied to compose service invocations.
  - `warning_emitted` (bool): Whether a warning was issued for missing GPUs in `detect` mode.
- **Constraints**: All application flags share the same `selected_mode` within one `up` call; warnings occur at most once per invocation.

## Relationships
- `GPUMode` informs `UpGpuApplication.selected_mode`.
- `HostGpuCapability` is produced when `GPUMode == detect` and influences whether GPU requests are sent in `UpGpuApplication`.
- `UpGpuApplication.warning_emitted` depends on `HostGpuCapability.available` and probe outcomes.

## State Transitions
1. **Mode selection**: User input → `GPUMode` (defaults to `none` if absent).
2. **Detection (detect mode only)**: Probe host → `HostGpuCapability`.
3. **Application**: Apply GPU request flags to run/build/compose paths based on `GPUMode` and `HostGpuCapability.available`.
4. **Warning emission**: If `detect` and no GPU available or probe fails → set `warning_emitted = true` before startup.
