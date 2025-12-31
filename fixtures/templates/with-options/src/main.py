#!/usr/bin/env python3
"""
Main application entry point.
Workspace location: ${localWorkspaceFolder}
"""

import os

def main():
    print(f"Hello from workspace: {os.environ.get('PWD', '${localWorkspaceFolder}')}")
    print("Application starting...")

if __name__ == "__main__":
    main()