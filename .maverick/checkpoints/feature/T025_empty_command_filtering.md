# T025: Filter Empty/Null Commands Implementation

## Task Description
**Task**: [US2] Filter empty/null commands using is_empty_command() in crates/core/src/container_lifecycle.rs

**Context**: This is part of User Story 2 - Feature Lifecycle Commands Execute Before User Commands.

**Requirements**:
1. Use the is_empty_command() helper function (created in Phase 2, T008)
2. Ensure aggregate_lifecycle_commands() filters out empty/null commands
3. This should be integrated into the T024 implementation

## Implementation Status: COMPLETE

### Summary

T025 was already implemented as part of T024. The `aggregate_lifecycle_commands()` function properly uses the `is_empty_command()` helper to filter out empty/null commands from both feature and config lifecycle commands.

### Implementation Details

#### Location
File: `/workspaces/deacon/crates/core/src/container_lifecycle.rs`

#### Key Functions

1. **is_empty_command()** (lines 85-98)
   - Checks if a command value is empty or null
   - Returns `true` for: `null`, `""`, `[]`, `{}`
   - Returns `false` for all other values

2. **aggregate_lifecycle_commands()** (lines 146-198)
   - Aggregates lifecycle commands from features and config
   - **Line 166**: Filters feature commands using `if !is_empty_command(cmd)`
   - **Line 189**: Filters config commands using `if !is_empty_command(cmd)`

### Code Evidence

```rust
// Line 85-98: The is_empty_command helper
pub fn is_empty_command(cmd: &serde_json::Value) -> bool {
    match cmd {
        // Null is empty
        serde_json::Value::Null => true,
        // Empty string is empty
        serde_json::Value::String(s) => s.is_empty(),
        // Empty array is empty
        serde_json::Value::Array(arr) => arr.is_empty(),
        // Empty object is empty
        serde_json::Value::Object(obj) => obj.is_empty(),
        // All other values are not empty
        _ => false,
    }
}

// Lines 165-174: Feature command filtering
if let Some(cmd) = cmd_opt {
    if !is_empty_command(cmd) {  // <-- T025: Empty command filtering
        commands.push(AggregatedLifecycleCommand {
            command: cmd.clone(),
            source: LifecycleCommandSource::Feature {
                id: feature.id.clone(),
            },
        });
    }
}

// Lines 188-195: Config command filtering
if let Some(cmd) = config_cmd_opt {
    if !is_empty_command(cmd) {  // <-- T025: Empty command filtering
        commands.push(AggregatedLifecycleCommand {
            command: cmd.clone(),
            source: LifecycleCommandSource::Config,
        });
    }
}
```

### Testing

#### Existing Tests
The `is_empty_command()` helper has comprehensive unit tests:
- `test_is_empty_command_null` - Tests null filtering
- `test_is_empty_command_empty_string` - Tests empty string filtering
- `test_is_empty_command_empty_array` - Tests empty array filtering
- `test_is_empty_command_empty_object` - Tests empty object filtering
- `test_is_empty_command_non_empty_string` - Tests non-empty strings pass through
- `test_is_empty_command_non_empty_array` - Tests non-empty arrays pass through
- `test_is_empty_command_non_empty_object` - Tests non-empty objects pass through
- Additional edge case tests for whitespace and nested structures

#### New Tests
Created comprehensive integration tests in `/workspaces/deacon/crates/core/tests/test_aggregate_lifecycle_commands.rs`:

1. **test_aggregate_lifecycle_commands_ordering** - Verifies correct command ordering
2. **test_aggregate_lifecycle_commands_filters_empty_null** - Tests null command filtering
3. **test_aggregate_lifecycle_commands_filters_empty_string** - Tests empty string filtering
4. **test_aggregate_lifecycle_commands_filters_empty_array** - Tests empty array filtering
5. **test_aggregate_lifecycle_commands_filters_empty_object** - Tests empty object filtering
6. **test_aggregate_lifecycle_commands_all_empty** - Tests when all commands are empty
7. **test_aggregate_lifecycle_commands_no_features** - Tests config-only scenario
8. **test_aggregate_lifecycle_commands_complex_command_formats** - Tests preservation of different command formats

### Verification Checklist

- [x] `is_empty_command()` helper function exists and is tested
- [x] `aggregate_lifecycle_commands()` uses `is_empty_command()` for feature commands
- [x] `aggregate_lifecycle_commands()` uses `is_empty_command()` for config commands
- [x] Empty null values are filtered out
- [x] Empty strings ("") are filtered out
- [x] Empty arrays ([]) are filtered out
- [x] Empty objects ({}) are filtered out
- [x] Non-empty commands are preserved
- [x] Unit tests verify is_empty_command() behavior
- [x] Integration tests verify end-to-end filtering in aggregate_lifecycle_commands()

### Contract Compliance

Per the lifecycle commands contract (`/workspaces/deacon/specs/009-complete-feature-support/contracts/lifecycle-commands.md`):

**Rule 3: Filter Empty Commands** (lines 107-113)
> Commands are filtered BEFORE aggregation if they are:
> - `null`
> - Empty string `""`
> - Empty array `[]`
> - Empty object `{}`

✅ **Implementation complies with contract** - All four empty command types are properly filtered using `is_empty_command()` before commands are added to the aggregated list.

### Files Modified

1. **Implementation**: `/workspaces/deacon/crates/core/src/container_lifecycle.rs`
   - Lines 85-98: `is_empty_command()` helper
   - Lines 146-198: `aggregate_lifecycle_commands()` with filtering
   - Lines 2614-2761: Unit tests for `is_empty_command()`

2. **Tests**: `/workspaces/deacon/crates/core/tests/test_aggregate_lifecycle_commands.rs`
   - New file with 8 comprehensive integration tests

### Conclusion

T025 is **COMPLETE**. The implementation properly filters empty/null commands using the `is_empty_command()` helper function as required. The filtering is applied to both feature and config commands before they are added to the aggregated command list, ensuring compliance with the lifecycle commands contract.
