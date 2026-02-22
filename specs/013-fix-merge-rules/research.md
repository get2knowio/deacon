# Research: Fix Config Merge Rules

**Branch**: `013-fix-merge-rules` | **Date**: 2026-02-22

## Decision 1: Upstream Spec Merge Semantics Confirmed

**Decision**: Implement boolean OR for `privileged`/`init` and array union for `mounts`/`forwardPorts` per upstream containers.dev spec.

**Rationale**: The upstream [containers.dev implementors spec](https://containers.dev/implementors/spec/) explicitly defines:
- `privileged`, `init`: "true if at least one is true, false otherwise"
- `forwardPorts`: "Union of all ports without duplicates. Last one wins (when mapping changes)."
- `mounts`: "Collected list of all mountpoints. Conflicts: Last source wins."

**Alternatives considered**:
- Keep last-wins for booleans: Rejected — violates spec, causes Features requiring `privileged: true` to silently lose that capability.
- Use replace for arrays: Rejected — violates spec, causes Features adding mounts/ports to silently destroy user entries.

## Decision 2: Fix Scope Limited to ConfigMerger::merge_two_configs

**Decision**: Only modify `ConfigMerger::merge_two_configs()` in `crates/core/src/config.rs`. Do not modify `mount::merge_mounts()` in `mount.rs`.

**Rationale**: These operate at different merge levels:
- `merge_two_configs()`: Merges two `DevContainerConfig` objects (extends chains, Feature metadata overlay). This is where the bug lives — booleans use `Option::or()` (last-wins) and arrays use replace-if-non-empty.
- `mount::merge_mounts()`: Merges config mounts with resolved Feature mounts by target path. This is a higher-level deduplication with different semantics (target-path-based, config overrides features). This function is correct for its purpose.

**Alternatives considered**:
- Modify both merge functions: Rejected — `merge_mounts` operates on a different abstraction level with intentionally different deduplication (by target path vs by JSON representation). Changing it would break Feature mount precedence.

## Decision 3: Boolean OR Implementation Strategy

**Decision**: Replace `overlay.privileged.or(base.privileged)` with a dedicated `merge_bool_or()` helper that implements: `Some(true)` if either is `Some(true)`, `Some(false)` if both are `Some(false)`, `None` if both are `None`, and pass-through for mixed `Some`/`None`.

**Rationale**: `Option::or()` returns the first `Some` value, meaning `Some(false).or(Some(true))` returns `Some(false)` — this is last-wins, not OR. The correct behavior is: if any source says `true`, the result is `true`.

**Alternatives considered**:
- Use `Option::max()` since `true > false`: This works for the `Some` cases but doesn't handle `None` correctly — `None.max(Some(false))` returns `Some(false)`, which is correct, but it's less readable and the intent is obscured. A named helper is clearer.
- Inline the logic: Rejected — used for two properties (`privileged`, `init`), DRY is appropriate.

## Decision 4: Array Union Implementation Strategy

**Decision**: Implement union as: start with base entries, then append overlay entries not already present. For mounts, compare using `serde_json::Value` equality (which compares full JSON structure). For forwardPorts, compare using `PortSpec::PartialEq` (already derived).

**Rationale**:
- `serde_json::Value` implements `PartialEq` with structural comparison, so two identical JSON objects/strings compare equal regardless of internal representation details.
- `PortSpec` already derives `PartialEq`, so `PortSpec::Number(3000) != PortSpec::String("3000:3000")` — they are different variants representing different things.
- Base-first ordering matches the spec: "the devcontainer.json is considered last" means user config entries appear first, Feature entries are appended.

**Alternatives considered**:
- Use `HashSet` for deduplication: Rejected — `serde_json::Value` doesn't implement `Hash`, and order preservation is required.
- Use `IndexSet` from `indexmap`: Rejected — again, `serde_json::Value` doesn't implement `Hash`. The `Vec::contains()` approach is O(n²) but mount/port arrays are always small (typically < 20 entries), so this is negligible.
- Serialize mounts to strings for comparison: Rejected — the spec edge case explicitly states string-form and object-form mounts are treated as distinct. JSON structural equality handles this correctly.

## Decision 5: No New Dependencies Required

**Decision**: No new crate dependencies needed.

**Rationale**: All required functionality exists:
- `serde_json::Value` already implements `PartialEq` for mount comparison
- `PortSpec` already derives `PartialEq` for port comparison
- `Vec::contains()` provides adequate deduplication for small arrays
- No `Hash` trait implementations needed since we don't use set data structures
