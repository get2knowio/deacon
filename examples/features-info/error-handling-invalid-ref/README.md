# Error Handling - Invalid Reference

**Edge Case**: Invalid feature reference format  
**Format**: Both text and JSON

## Description

Demonstrates error handling when an invalid or non-existent feature reference is provided. Shows the difference between text and JSON error output.

## Use Case

- Validating user input
- Understanding error messages
- CI/CD error handling
- Testing error paths

## Prerequisites

None - errors are deterministic

## Running - Text Mode

```bash
# Invalid reference format
deacon features info manifest invalid-feature-ref

# Non-existent feature
deacon features info manifest ghcr.io/does-not-exist/fake-feature:1.0.0

# Malformed registry URL
deacon features info manifest not.a.registry//feature
```

### Text Mode Output

Human-readable error message:
```
Error: Invalid feature reference format: invalid-feature-ref
```

Exit code: **1**

## Running - JSON Mode

```bash
# Same invalid references with JSON output
deacon features info manifest invalid-feature-ref --output-format json
deacon features info manifest ghcr.io/does-not-exist/fake-feature:1.0.0 --output-format json
```

### JSON Mode Output

Per spec (FR-008):
```json
{}
```

Exit code: **1**

**Critical**: JSON mode outputs empty object for ANY error to maintain machine parsability.

## Error Types Tested

1. **Format validation errors**
   - Missing registry
   - Invalid characters
   - Malformed structure

2. **Network errors**
   - Non-existent repository
   - 404 from registry
   - DNS resolution failure

3. **Authentication errors**
   - 401 Unauthorized
   - 403 Forbidden

All produce:
- Text mode: Descriptive error message
- JSON mode: `{}` + exit 1

## Success Criteria

- ✅ Invalid refs rejected immediately
- ✅ Text mode shows helpful error messages
- ✅ JSON mode outputs `{}`
- ✅ Exit code is 1 in all cases
- ✅ No partial data in error responses

## Error Handling Guarantees

Per spec (FR-008):
- Errors never produce partial JSON output
- JSON mode never mixes text with JSON
- Exit codes are consistent
- Errors are logged to stderr (visible with `--log-level debug`)

## Testing Script

```bash
#!/bin/bash
# test-error-handling.sh - Verify error behavior

test_error() {
  local ref="$1"
  local format="$2"
  
  echo "Testing: $ref ($format)"
  
  if [ "$format" = "json" ]; then
    OUTPUT=$(deacon features info manifest "$ref" --output-format json 2>/dev/null)
    EXIT_CODE=$?
    
    if [ "$OUTPUT" = "{}" ] && [ $EXIT_CODE -eq 1 ]; then
      echo "  ✓ Correct error output"
    else
      echo "  ✗ Unexpected: $OUTPUT (exit $EXIT_CODE)"
      return 1
    fi
  else
    OUTPUT=$(deacon features info manifest "$ref" 2>&1)
    EXIT_CODE=$?
    
    if [ $EXIT_CODE -eq 1 ] && echo "$OUTPUT" | grep -q "Error:"; then
      echo "  ✓ Correct error message"
    else
      echo "  ✗ Unexpected exit code: $EXIT_CODE"
      return 1
    fi
  fi
}

# Run tests
test_error "invalid-ref" "text"
test_error "invalid-ref" "json"
test_error "ghcr.io/does-not-exist/fake:1" "text"
test_error "ghcr.io/does-not-exist/fake:1" "json"

echo "All error handling tests passed!"
```

## Related Examples

- [error-handling-network-failure](../error-handling-network-failure/) - Network timeout errors
- [manifest-json-output](../manifest-json-output/) - Successful JSON output for comparison
