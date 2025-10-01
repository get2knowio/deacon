#!/bin/bash
set -e

echo "Installing feature-with-options..."

# Example: Use the options (these would be substituted during feature installation)
ENABLE_FEATURE="${ENABLEFEATURE:-true}"
VERSION="${VERSION:-stable}"
CUSTOM_PATH="${CUSTOMPATH:-/usr/local}"
DEBUG_MODE="${DEBUGMODE:-false}"

echo "Configuration:"
echo "  Enable Feature: $ENABLE_FEATURE"
echo "  Version: $VERSION"
echo "  Custom Path: $CUSTOM_PATH"
echo "  Debug Mode: $DEBUG_MODE"

# Example installation steps
if [ "$ENABLE_FEATURE" = "true" ]; then
    echo "Feature enabled - performing installation..."
    mkdir -p "$CUSTOM_PATH/feature"
    echo "version=$VERSION" > "$CUSTOM_PATH/feature/version.txt"
    
    if [ "$DEBUG_MODE" = "true" ]; then
        echo "Debug mode enabled"
    fi
fi

echo "Feature installation complete!"
