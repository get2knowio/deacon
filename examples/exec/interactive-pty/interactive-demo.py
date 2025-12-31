#!/usr/bin/env python3
"""Interactive demonstration script requiring PTY for proper operation."""

import sys

def main():
    print("=== Interactive Input Demo ===")
    print("This script demonstrates interactive input in PTY mode.\n")
    
    # Check if we have a TTY
    if sys.stdin.isatty():
        print("✓ Running with TTY (PTY mode)")
    else:
        print("✗ No TTY detected (non-interactive mode)")
    
    # Try to get interactive input
    try:
        name = input("\nEnter your name: ")
        print(f"Hello, {name}!")
        
        # Demonstrate colored output (requires PTY)
        print("\n\033[32mGreen text (requires PTY)\033[0m")
        print("\033[33mYellow text (requires PTY)\033[0m")
        print("\033[34mBlue text (requires PTY)\033[0m")
        
        confirm = input("\nType 'yes' to continue: ")
        if confirm.lower() == 'yes':
            print("✓ Confirmation received")
        else:
            print("✗ Not confirmed")
            
    except EOFError:
        print("\n✗ EOF encountered (possibly no PTY)")
        return 1
    except KeyboardInterrupt:
        print("\n✗ Interrupted by user (Ctrl+C)")
        return 130  # 128 + SIGINT(2)
    
    print("\n=== Demo Complete ===")
    return 0

if __name__ == "__main__":
    sys.exit(main())
