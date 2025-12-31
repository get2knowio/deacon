#!/bin/bash
# Long-running script for timeout testing

echo "=== Timeout Test Script ==="
echo "Process ID: $$"
echo "This script will run indefinitely until terminated."
echo ""

# Infinite loop
COUNT=0
while true; do
    COUNT=$((COUNT + 1))
    echo "Iteration $COUNT ($(date '+%H:%M:%S'))"
    sleep 2
done
