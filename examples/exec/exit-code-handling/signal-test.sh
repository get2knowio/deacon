#!/bin/bash
# Script for testing signal-based termination

echo "=== Signal Test Script ==="
echo "Process ID: $$"
echo ""
echo "This script will run for 60 seconds."
echo "Test signal handling by:"
echo "  - Pressing Ctrl+C (SIGINT → exit 130)"
echo "  - Sending SIGTERM: kill $$ (→ exit 143)"
echo "  - Sending SIGKILL: kill -9 $$ (→ exit 137)"
echo ""

# Trap signals to show when they're received
trap 'echo "Received SIGTERM"; exit 0' TERM
trap 'echo "Received SIGINT"; exit 0' INT
trap 'echo "Received SIGHUP"; exit 0' HUP

# Count up for 60 seconds
for i in {1..60}; do
    echo "Running... $i/60 seconds"
    sleep 1
done

echo "Script completed normally"
exit 0
