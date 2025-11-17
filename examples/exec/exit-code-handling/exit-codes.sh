#!/bin/bash
# Script demonstrating various exit code scenarios

show_usage() {
    echo "Usage: $0 <exit_code|test-all>"
    echo ""
    echo "Examples:"
    echo "  $0 0          # Exit with success"
    echo "  $0 1          # Exit with generic failure"
    echo "  $0 42         # Exit with custom code"
    echo "  $0 test-all   # Run all test scenarios"
}

test_all() {
    echo "=== Exit Code Test Suite ==="
    echo ""
    
    echo "Test 1: Success (exit 0)"
    bash -c 'exit 0'
    echo "Result: $?"
    echo ""
    
    echo "Test 2: Generic failure (exit 1)"
    bash -c 'exit 1'
    echo "Result: $?"
    echo ""
    
    echo "Test 3: Custom exit code (exit 42)"
    bash -c 'exit 42'
    echo "Result: $?"
    echo ""
    
    echo "Test 4: Command not found (exit 127)"
    nonexistent_command_xyz 2>/dev/null
    echo "Result: $?"
    echo ""
    
    echo "Test 5: Permission denied simulation (exit 126)"
    bash -c 'exit 126'
    echo "Result: $?"
    echo ""
    
    echo "=== All tests complete ==="
    return 0
}

# Main script
if [ $# -eq 0 ]; then
    show_usage
    exit 1
fi

case "$1" in
    test-all)
        test_all
        exit 0
        ;;
    -h|--help)
        show_usage
        exit 0
        ;;
    *)
        if [[ "$1" =~ ^[0-9]+$ ]]; then
            echo "Exiting with code: $1"
            exit "$1"
        else
            echo "Error: Invalid exit code '$1'"
            show_usage
            exit 1
        fi
        ;;
esac
