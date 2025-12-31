#!/bin/bash
# Terminal capabilities test script

echo "=== Terminal Test ==="
echo ""

# Check if we have a TTY
if [ -t 0 ] && [ -t 1 ]; then
    echo "✓ stdin and stdout are TTYs"
    TTY_PATH=$(tty)
    echo "  TTY device: $TTY_PATH"
else
    echo "✗ Not running with full TTY"
fi

# Check terminal size
if command -v tput &> /dev/null; then
    COLS=$(tput cols 2>/dev/null || echo "unknown")
    ROWS=$(tput lines 2>/dev/null || echo "unknown")
    echo ""
    echo "Terminal dimensions:"
    echo "  Columns: $COLS"
    echo "  Rows: $ROWS"
    echo "  COLUMNS env: ${COLUMNS:-not-set}"
    echo "  LINES env: ${LINES:-not-set}"
fi

# Test colors (requires PTY)
echo ""
echo "Color test (requires PTY):"
echo -e "  \033[31mRed\033[0m"
echo -e "  \033[32mGreen\033[0m"
echo -e "  \033[33mYellow\033[0m"
echo -e "  \033[34mBlue\033[0m"
echo -e "  \033[35mMagenta\033[0m"
echo -e "  \033[36mCyan\033[0m"

# Test cursor positioning (requires PTY)
echo ""
echo "Cursor positioning test:"
echo -n "  [    ] Loading"
sleep 0.5
echo -ne "\r  [=   ] Loading"
sleep 0.5
echo -ne "\r  [==  ] Loading"
sleep 0.5
echo -ne "\r  [=== ] Loading"
sleep 0.5
echo -ne "\r  [====] Complete\n"

echo ""
echo "=== Test Complete ==="
