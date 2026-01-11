#!/bin/bash

# Script to verify merge_security_options implementation

set -e

echo "=== Testing merge_security_options implementation ==="
echo ""

echo "Step 1: Building deacon-core..."
cargo build --package deacon-core --lib

echo ""
echo "Step 2: Running merge_security_options tests..."
cargo nextest run -p deacon-core merge_security_options

echo ""
echo "=== All tests passed! ==="
