#!/usr/bin/env python3
"""Sample application to demonstrate exec with label targeting."""

import os
import sys

def main():
    print("=== Python Application Execution ===")
    print(f"Python version: {sys.version}")
    print(f"Executable: {sys.executable}")
    print(f"Current user: {os.getenv('USER', 'unknown')}")
    print(f"Working directory: {os.getcwd()}")
    print(f"APP_NAME env: {os.getenv('APP_NAME', 'not-set')}")
    print("=== Execution Complete ===")
    return 0

if __name__ == "__main__":
    sys.exit(main())
