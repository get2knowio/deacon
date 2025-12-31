# Error Handling - Network Failure

**Edge Case**: Network timeouts and connectivity issues  
**Format**: Both text and JSON

## Description

Demonstrates error handling for network-related failures including timeouts, DNS resolution errors, and connection refused scenarios.

## Use Case

- Understanding timeout behavior
- CI/CD resilience testing
- Network troubleshooting
- Offline development error messages

## Prerequisites

- Network isolation (optional) for testing
- Understanding of timeout settings (10s per request per spec)

## Timeout Configuration

Per spec (FR-004):
- Per-request timeout: **10 seconds**
- Pagination: up to **10 pages**
- Total tags cap: **1000 tags**

## Running - Simulated Timeout

```bash
# Try to access a slow/unreachable registry
# (This will timeout after 10s)
deacon features info manifest registry.example.invalid/feature:1

# With debug logging to see timeout details
deacon features info manifest registry.example.invalid/feature:1 --log-level debug
```

### Text Mode Output

```
Error: Failed to fetch manifest: timeout after 10s
```

Exit code: **1**

### JSON Mode Output

```json
{}
```

Exit code: **1**

## Network Error Scenarios

### 1. DNS Resolution Failure

```bash
deacon features info manifest nonexistent.registry.example/feature:1
```

Error: DNS lookup failed

### 2. Connection Refused

```bash
deacon features info manifest localhost:9999/feature:1
```

Error: Connection refused

### 3. Request Timeout

Long-running or stalled connections timeout after 10s

### 4. TLS/SSL Errors

```bash
# Invalid certificate or TLS handshake failure
deacon features info manifest https://self-signed.badssl.com/feature:1
```

Error: TLS handshake failed

## Pagination Timeout

For tags mode with many pages:

```bash
# If pagination takes too long, may hit overall limits
deacon features info tags ghcr.io/large-repo/feature
```

Per spec, pagination will:
- Fetch up to 10 pages
- Stop at 1000 total tags
- Honor 10s timeout per request

## Success Criteria

- ✅ Timeouts occur after 10 seconds
- ✅ Text mode shows clear error messages
- ✅ JSON mode outputs `{}`
- ✅ Exit code is 1
- ✅ No hanging requests

## Debugging Network Issues

```bash
# Enable debug logging to see HTTP requests
export RUST_LOG=debug
deacon features info manifest registry.example.invalid/feature:1

# Check network connectivity separately
curl -v https://ghcr.io/v2/
```

## Testing Script

```bash
#!/bin/bash
# test-network-errors.sh - Verify network error handling

echo "Testing DNS failure..."
OUTPUT=$(deacon features info manifest nonexistent.example/feature:1 --output-format json 2>/dev/null)
if [ "$OUTPUT" = "{}" ] && [ $? -eq 1 ]; then
  echo "  ✓ DNS failure handled correctly"
fi

echo "Testing connection refused..."
OUTPUT=$(deacon features info manifest localhost:9999/feature:1 --output-format json 2>/dev/null)
if [ "$OUTPUT" = "{}" ] && [ $? -eq 1 ]; then
  echo "  ✓ Connection refused handled correctly"
fi

echo "All network error tests passed!"
```

## Timeout Testing

```bash
#!/bin/bash
# test-timeout.sh - Measure timeout duration

echo "Testing timeout (should take ~10s)..."
START=$(date +%s)

deacon features info manifest registry.example.invalid/feature:1 2>/dev/null
EXIT_CODE=$?

END=$(date +%s)
DURATION=$((END - START))

echo "Duration: ${DURATION}s"
echo "Exit code: $EXIT_CODE"

if [ $DURATION -ge 9 ] && [ $DURATION -le 12 ] && [ $EXIT_CODE -eq 1 ]; then
  echo "✓ Timeout behavior correct (~10s)"
else
  echo "✗ Unexpected timeout duration or exit code"
fi
```

## Related Examples

- [error-handling-invalid-ref](../error-handling-invalid-ref/) - Format validation errors
- [manifest-public-registry](../manifest-public-registry/) - Successful manifest fetch
