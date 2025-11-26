#!/bin/bash
# sync-branch.sh
# Syncs current branch with origin/main and returns structured JSON
# Returns: {"status": "ok"|"conflicts", "branch": "...", "spec_dir": "...", "tasks_file": "...", "conflicts?": "..."}

set -e

BRANCH_NAME=$(git branch --show-current)
SPEC_DIR="specs/$BRANCH_NAME"
TASKS_FILE="$SPEC_DIR/tasks.md"

# Fetch latest
git fetch origin 2>/dev/null

# Attempt rebase
if ! git rebase origin/main 2>/dev/null; then
    CONFLICTS=$(git diff --name-only --diff-filter=U | tr '\n' ' ')
    cat <<EOF
{
  "status": "conflicts",
  "branch": "$BRANCH_NAME",
  "spec_dir": "$SPEC_DIR",
  "tasks_file": "$TASKS_FILE",
  "conflicts": "$CONFLICTS"
}
EOF
    exit 1
fi

# Check if spec directory exists
if [ ! -d "$SPEC_DIR" ]; then
    cat <<EOF
{
  "status": "error",
  "branch": "$BRANCH_NAME",
  "spec_dir": "$SPEC_DIR",
  "tasks_file": "$TASKS_FILE",
  "error": "Spec directory does not exist: $SPEC_DIR"
}
EOF
    exit 1
fi

# Check if tasks file exists
if [ ! -f "$TASKS_FILE" ]; then
    cat <<EOF
{
  "status": "error",
  "branch": "$BRANCH_NAME",
  "spec_dir": "$SPEC_DIR",
  "tasks_file": "$TASKS_FILE",
  "error": "Tasks file does not exist: $TASKS_FILE"
}
EOF
    exit 1
fi

cat <<EOF
{
  "status": "ok",
  "branch": "$BRANCH_NAME",
  "spec_dir": "$SPEC_DIR",
  "tasks_file": "$TASKS_FILE"
}
EOF
