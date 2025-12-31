#!/bin/bash
# Binary-safe I/O test

echo "=== Binary I/O Test ===" >&2

# Test 1: Read and verify binary file
if [ -f /workspace/data/sample.bin ]; then
    SIZE=$(stat -c%s /workspace/data/sample.bin 2>/dev/null || stat -f%z /workspace/data/sample.bin 2>/dev/null)
    echo "Binary file size: $SIZE bytes" >&2
    
    # Output first 16 bytes in hex
    echo "First 16 bytes (hex):" >&2
    xxd -l 16 /workspace/data/sample.bin >&2
fi

# Test 2: Generate and output binary data
echo "" >&2
echo "Generating binary sequence..." >&2
# Output bytes 0-255 to stdout
for i in {0..255}; do
    printf "\\x$(printf %02x $i)"
done

echo "" >&2
echo "=== Test Complete ===" >&2
