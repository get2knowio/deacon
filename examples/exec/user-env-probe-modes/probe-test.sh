#!/bin/bash
# Environment probe test script

echo "=== Environment Probe Test ==="
echo ""

# Check which init files were sourced
echo "Shell initialization markers:"
echo "  PROFILE_LOADED:      ${PROFILE_LOADED:-not-set}"
echo "  BASHRC_LOADED:       ${BASHRC_LOADED:-not-set}"
echo "  CUSTOM_VAR:          ${CUSTOM_VAR:-not-set}"
echo ""

# Display PATH
echo "PATH environment:"
echo "$PATH" | tr ':' '\n' | nl
echo ""

# Check for common tools
echo "Tool availability:"
for tool in git node python3 npm cargo go; do
    if command -v $tool &> /dev/null; then
        LOCATION=$(command -v $tool)
        echo "  ✓ $tool: $LOCATION"
    else
        echo "  ✗ $tool: not found"
    fi
done
echo ""

# Environment variable count
echo "Environment variable count: $(env | wc -l)"
echo ""

# Shell info
echo "Shell information:"
echo "  SHELL:     $SHELL"
echo "  BASH:      ${BASH_VERSION:-not-bash}"
echo "  ZSH:       ${ZSH_VERSION:-not-zsh}"
echo ""

# Check if this is a login shell
if shopt -q login_shell 2>/dev/null; then
    echo "  Login shell: yes"
else
    echo "  Login shell: no"
fi

# Check if this is interactive
if [[ $- == *i* ]]; then
    echo "  Interactive: yes"
else
    echo "  Interactive: no"
fi

echo ""
echo "=== Test Complete ==="
