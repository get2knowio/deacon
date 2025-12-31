#!/bin/bash
# Script that produces both stdout and stderr output

echo "=== Output Generation Test ===" >&2
echo "Starting at $(date)" >&2
echo ""

# Produce lines to stdout
for i in {1..10}; do
    echo "stdout line $i"
    
    # Occasionally write to stderr
    if [ $((i % 3)) -eq 0 ]; then
        echo "stderr: progress checkpoint at line $i" >&2
    fi
    
    sleep 0.1
done

echo ""
echo "Completed at $(date)" >&2
echo "=== Test Complete ===" >&2

# Final output to stdout
echo "Total lines: 10"
