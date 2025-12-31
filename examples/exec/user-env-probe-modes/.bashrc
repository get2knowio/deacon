# .bashrc - Bash initialization for interactive shells

# Mark that this file was loaded
export BASHRC_LOADED="yes"

# Custom variable to demonstrate interactive shell initialization
export CUSTOM_VAR="set-by-bashrc"

# Custom PATH modification (simulating tool installation)
export PATH="$HOME/.local/bin:$PATH"

# Aliases (only useful in interactive mode)
alias ll='ls -la'
alias gs='git status'

# Custom prompt (interactive only)
if [ -n "$PS1" ]; then
    PS1='\u@\h:\w\$ '
fi

# Load additional configuration if present
if [ -f "$HOME/.bash_aliases" ]; then
    . "$HOME/.bash_aliases"
fi

echo "[.bashrc loaded]" >&2
