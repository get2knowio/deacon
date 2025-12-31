#!/bin/bash
# sync-branch.sh
# Syncs current branch with origin/main and returns structured JSON
# Usage: sync-branch.sh [branch-name]
#   If branch-name is provided, switches to that branch first
#   If not provided, uses the current branch
# Returns: {"status": "ok"|"conflicts", "branch": "...", "spec_dir": "...", "tasks_file": "...", "conflicts?": "..."}

set -e

# If a branch argument is provided, switch to it first
if [ -n "$1" ]; then
    TARGET_BRANCH="$1"

    # Fetch to ensure we have latest refs
    git fetch origin 2>/dev/null

    # Check if we need to switch branches
    CURRENT=$(git branch --show-current)
    if [ "$CURRENT" != "$TARGET_BRANCH" ]; then
        # Try to switch to the branch
        if ! git checkout "$TARGET_BRANCH" 2>/dev/null; then
            # Branch might not exist locally, try to check out from origin
            if ! git checkout -b "$TARGET_BRANCH" "origin/$TARGET_BRANCH" 2>/dev/null; then
                cat <<EOF
{
  "status": "error",
  "branch": "$TARGET_BRANCH",
  "spec_dir": "specs/$TARGET_BRANCH",
  "tasks_file": "specs/$TARGET_BRANCH/tasks.md",
  "error": "Could not switch to branch: $TARGET_BRANCH"
}
EOF
                exit 1
            fi
        fi
    fi
fi

BRANCH_NAME=$(git branch --show-current)
SPEC_DIR="specs/$BRANCH_NAME"
TASKS_FILE="$SPEC_DIR/tasks.md"

# Fetch latest
git fetch origin 2>/dev/null

# Check if we have local commits not in origin/main
LOCAL_COMMITS=$(git rev-list origin/main..HEAD 2>/dev/null | wc -l | tr -d ' ')

# Attempt rebase
if ! git rebase origin/main 2>/dev/null; then
    CONFLICTS=$(git diff --name-only --diff-filter=U | tr '\n' ' ')
    cat <<EOF
{
  "status": "conflicts",
  "branch": "$BRANCH_NAME",
  "spec_dir": "$SPEC_DIR",
  "tasks_file": "$TASKS_FILE",
  "conflicts": "$CONFLICTS",
  "local_commits": $LOCAL_COMMITS
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
  "tasks_file": "$TASKS_FILE",
  "local_commits": $LOCAL_COMMITS,
  "synced_with": "origin/main"
}
EOF
