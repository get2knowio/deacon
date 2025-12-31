#!/bin/bash
# User identity and permissions verification script

echo "=== User Identity Check ==="
echo ""

# Current user
echo "Current user:"
echo "  whoami:    $(whoami)"
echo "  \$USER:     $USER"
echo "  \$HOME:     $HOME"
echo "  \$SHELL:    $SHELL"
echo ""

# User ID details
echo "User ID details:"
id
echo ""

# Group memberships
echo "Group memberships:"
groups
echo ""

# Home directory
echo "Home directory contents:"
ls -la $HOME 2>/dev/null | head -n 10
echo ""

# File creation permissions
echo "Testing file creation in workspace:"
TEST_FILE="/workspace/shared-workspace/.user-test-$$"
if touch "$TEST_FILE" 2>/dev/null; then
    ls -l "$TEST_FILE"
    rm "$TEST_FILE"
    echo "✓ Can create files in workspace"
else
    echo "✗ Cannot create files in workspace"
fi
echo ""

# Check sudo availability
echo "Sudo access:"
if command -v sudo &> /dev/null; then
    if sudo -n true 2>/dev/null; then
        echo "✓ Passwordless sudo available"
    else
        echo "✗ Sudo requires password or not available"
    fi
else
    echo "✗ Sudo not installed"
fi
echo ""

# Process ownership
echo "Process info:"
ps -o user,pid,comm -p $$ | head -n 2
echo ""

echo "=== Check Complete ==="
