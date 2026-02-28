# Research: Fix Lifecycle Command Format Support

**Feature Branch**: `012-fix-lifecycle-formats`
**Date**: 2026-02-21

## Decision 1: Command Format Representation

**Question**: How should lifecycle command formats be represented internally to preserve format semantics through the execution pipeline?

**Decision**: Introduce a `LifecycleCommandValue` enum with three variants (`Shell(String)`, `Exec(Vec<String>)`, `Parallel(IndexMap<String, LifecycleCommandValue>)`) to replace the current `serde_json::Value`-to-`Vec<String>` flattening.

**Rationale**: The current implementation in `commands_from_json_value()` flattens all formats to `Vec<String>`, losing the distinction between shell-interpreted strings, exec-style arrays, and parallel object commands. The execution layer (`execute_lifecycle_phase_impl`) then wraps everything in `sh -c`, defeating exec-style semantics. A typed enum preserves format intent through the entire pipeline.

**Alternatives considered**:
- Keep `serde_json::Value` and branch at execution time — rejected because variable substitution currently operates on stringified JSON, corrupting array/object formats
- Use `Vec<(String, ExecutionStyle)>` pairs — rejected because it doesn't naturally represent nested object values (string vs array within an object)

## Decision 2: Exec-Style Container Execution

**Question**: How should array-format commands be executed in containers without shell wrapping?

**Decision**: Pass array elements directly to `Docker::exec()` as the command args, bypassing the `sh -c` wrapper. The first element is the executable, remaining elements are arguments.

**Rationale**: The DevContainer spec states arrays are "passed to the OS for execution without going through a shell." The current implementation shell-quotes and joins array elements into a single string passed to `sh -c`, which defeats the purpose of exec-style invocation (e.g., arguments with spaces would need escaping).

**Alternatives considered**:
- Shell-quote and join (current approach) — rejected because it's semantically wrong; arguments like `"hello world"` should be a single arg, not parsed by shell
- Use `docker exec` with `--` separator — rejected; unnecessary, direct arg passing works

## Decision 3: Parallel Execution Strategy for Object Format

**Question**: How should object-format commands execute concurrently?

**Decision**: Use two different concurrency strategies depending on the execution context. The host-side path (`execute_host_lifecycle_phase`) uses `tokio::JoinSet` with `spawn_blocking` to run one blocking `std::process::Command` per object entry concurrently. The container-side path (`execute_lifecycle_phase_impl`) uses `futures::future::join_all` to await all Docker exec futures concurrently. In both paths, all tasks run to completion, output is prefixed with the command's named key for attribution, and failure of any entry fails the phase.

**Rationale**: The DevContainer spec defines object format for parallel execution. The two paths require different concurrency primitives because of their execution models. The host-side path spawns OS processes via `std::process::Command`, which is blocking -- `tokio::JoinSet` with `spawn_blocking` offloads each blocking call to a thread while keeping the async runtime responsive. The container-side path calls Docker exec through an async API, so the futures are already non-blocking -- `futures::future::join_all` is sufficient and simpler since cancellation semantics are not needed (all entries must complete per the spec). The spec requires all entries to complete before the phase is done and failure of any entry fails the phase.

**Alternatives considered**:
- `tokio::join!` macro — rejected because the number of entries is dynamic
- `tokio::JoinSet` for both paths — rejected for container-side; Docker exec calls are already async, so `join_all` is simpler and avoids unnecessary task spawning overhead
- `futures::join_all` for both paths — rejected for host-side; `std::process::Command` is blocking and would block the async runtime without `spawn_blocking`
- Sequential execution with labels — rejected; violates spec requirement for concurrent execution

## Decision 4: Host-Side Object Format Support

**Question**: How should object-format commands execute on the host (initializeCommand)?

**Decision**: Use `std::thread::scope` (or `tokio::task::spawn_blocking` with `JoinSet`) to run parallel host commands. Each entry spawns a process via `std::process::Command` (string through `sh -c`, array as direct exec). The host execution path in `lifecycle.rs` (`LifecycleCommands::from_json_value`) must be extended to support the object format.

**Rationale**: Host-side execution currently only handles string and array formats, rejecting objects with an error. Since `initializeCommand` runs on the host before container creation, it needs the same three-format support. The host path uses synchronous `std::process::Command`, so parallel execution uses thread-based concurrency.

**Alternatives considered**:
- Convert host path to fully async — rejected; unnecessary refactor for this feature, `spawn_blocking` is sufficient
- Execute object entries sequentially on host — rejected; violates spec

## Decision 5: Variable Substitution for Structured Commands

**Question**: How should variable substitution work for array and object command formats?

**Decision**: Apply variable substitution element-by-element for arrays (substitute each string element independently) and recursively for objects (substitute each value's string/array elements). Do not stringify the entire structure before substitution.

**Rationale**: The current code calls `VariableSubstitution::substitute_string` on a JSON-stringified command, which corrupts array/object structure. Element-wise substitution preserves the command structure while still resolving `${containerWorkspaceFolder}` and other variables in each string component.

**Alternatives considered**:
- Substitute on the raw `serde_json::Value` before parsing to typed enum — feasible but adds a JSON-level substitution pass; cleaner to substitute after parsing to typed form
- Skip substitution for array/object — rejected; spec allows variables in all formats

## Decision 6: Where to Place Format-Aware Execution Logic

**Question**: Should format-aware execution live in `crates/core/` or `crates/deacon/`?

**Decision**: Place the `LifecycleCommandValue` type and format-aware execution logic in `crates/core/src/container_lifecycle.rs` (container path) and `crates/core/src/lifecycle.rs` (host path). The parsing from `serde_json::Value` to `LifecycleCommandValue` also lives in core. The `crates/deacon/` layer (`up/lifecycle.rs`) delegates to core rather than doing its own format parsing.

**Rationale**: Format handling is domain logic, not CLI orchestration. Keeping it in core ensures `run-user-commands` and any future commands that execute lifecycle phases get format support automatically. The current `commands_from_json_value` in the `up` module duplicates parsing logic that should be centralized.

**Alternatives considered**:
- Keep parsing in `crates/deacon/` and pass typed commands to core — rejected; core already receives `serde_json::Value` via `AggregatedLifecycleCommand` and should handle the conversion
- Create a new `crates/core/src/lifecycle_format.rs` module — considered but the types integrate tightly with existing execution code; a new module is warranted if the file grows too large

## Decision 7: Progress Events and Output Attribution

**Question**: How should parallel command progress events and output be attributed?

**Decision**: Each parallel command entry gets its own `command_id` in the format `{phase}-{key}` (e.g., `postCreate-install`). Progress events (`LifecycleCommandBegin`, `LifecycleCommandEnd`) are emitted per entry. Output from parallel commands is prefixed with `[{key}]` for diagnostic clarity.

**Rationale**: FR-012 requires output attribution for parallel commands. Using the object key as the identifier aligns with how users define the commands and makes logs actionable. The existing progress event infrastructure supports per-command events; it just needs the key-based naming.

**Alternatives considered**:
- Emit a single compound event for the entire object — rejected; loses per-command attribution
- Use numeric indices — rejected; named keys from the object are more meaningful

## Decision 8: Error Behavior for Parallel Commands

**Question**: When one parallel command fails, what happens to the others?

**Decision**: Wait for all commands to complete (do not cancel siblings). Collect all results. If any command failed, the phase reports failure. Error messages include all failed command keys and their exit codes.

**Rationale**: The spec states the phase waits for all commands to complete, then reports failure. Cancelling siblings could leave partial state (e.g., a half-installed dependency). The spec edge case explicitly states: "the phase waits for all commands to complete, then reports failure."

**Alternatives considered**:
- Cancel siblings on first failure — rejected; spec says wait for all
- Continue to next phase if only some fail — rejected; spec says phase fails
