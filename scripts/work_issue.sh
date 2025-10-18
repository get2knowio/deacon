#!/usr/bin/env bash
set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Global flag for interactive mode
INTERACTIVE_MODE=false

# Function to print colored messages
log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

# Function to pause for user confirmation in interactive mode
pause_if_interactive() {
    if [ "$INTERACTIVE_MODE" = true ]; then
        echo
        read -p "Press Enter to continue..." -r
        echo
    fi
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check required commands
check_requirements() {
    local missing=()
    
    if ! command_exists git; then
        missing+=("git")
    fi
    
    if ! command_exists gh; then
        missing+=("gh")
    fi
    
    if ! command_exists copilot; then
        missing+=("copilot")
    fi
    
    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing required commands: ${missing[*]}"
        log_info "Please install the missing commands and try again."
        exit 1
    fi
}

# Function to check if we're in a git repository
check_git_repo() {
    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        log_error "Not in a git repository"
        exit 1
    fi
}

# Function to check if we have uncommitted changes
check_clean_working_tree() {
    if ! git diff-index --quiet HEAD -- 2>/dev/null; then
        log_warning "You have uncommitted changes in your working tree."
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Aborting."
            exit 1
        fi
    fi
}

# Function to fetch issue details
fetch_issue() {
    local issue_id="$1"
    log_info "Fetching issue #${issue_id}..."
    
    if ! gh issue view "$issue_id" --json title,body >/dev/null 2>&1; then
        log_error "Failed to fetch issue #${issue_id}"
        exit 1
    fi
    
    log_success "Issue #${issue_id} found"
}

# Function to create and switch to branch
create_branch() {
    local issue_id="$1"
    local branch_name="fix/issue-${issue_id}"
    
    log_info "Creating branch: ${branch_name}" >&2
    
    # Ensure we're on the default branch
    local default_branch
    default_branch=$(gh repo view --json defaultBranchRef --jq '.defaultBranchRef.name')
    
    log_info "Switching to default branch: ${default_branch}" >&2
    git checkout "$default_branch" >&2
    git pull origin "$default_branch" >&2
    
    # Create and switch to new branch
    if git rev-parse --verify "$branch_name" >/dev/null 2>&1; then
        log_warning "Branch ${branch_name} already exists. Switching to it." >&2
        git checkout "$branch_name" >&2
    else
        git checkout -b "$branch_name" >&2
        log_success "Created and switched to branch: ${branch_name}" >&2
        
        # Create an initial empty commit so we can push and create PR
        git commit --allow-empty -m "Initial commit for issue #${issue_id}" >&2
        log_success "Created initial commit" >&2
    fi
    
    echo "$branch_name"
}

# Function to create initial PR
create_pr() {
    local issue_id="$1"
    local branch_name="$2"
    
    log_info "Creating initial PR for issue #${issue_id}..."
    
    # Fetch issue details
    local issue_title
    local issue_body
    issue_title=$(gh issue view "$issue_id" --json title --jq '.title')
    issue_body=$(gh issue view "$issue_id" --json body --jq '.body')
    
    # Create PR body
    local pr_body
    pr_body=$(cat <<EOF
## Issue Description

${issue_body}

---

Fixes #${issue_id}
EOF
)
    
    # Push the branch to remote first
    log_info "Pushing branch to remote..."
    git push -u origin "$branch_name"
    
    # Check if PR already exists
    local existing_pr
    existing_pr=$(gh pr list --head "$branch_name" --json number --jq '.[0].number' 2>/dev/null || echo "")
    
    if [ -n "$existing_pr" ]; then
        log_warning "PR #${existing_pr} already exists for branch ${branch_name}"
        echo "$existing_pr"
    else
        # Create PR
        local pr_number
        pr_number=$(gh pr create --title "Fix: ${issue_title}" --body "$pr_body" --draft | grep -oP '(?<=pull/)[0-9]+')
        log_success "Created draft PR #${pr_number}"
        echo "$pr_number"
    fi
}

# Function to run copilot command
run_copilot() {
    local prompt="$1"
    log_info "Running: copilot --prompt ${prompt}"
    
    if copilot --prompt "$prompt"; then
        log_success "Completed: ${prompt}"
    else
        log_error "Failed: ${prompt}"
        return 1
    fi
}

# Function to wait for workflow to complete
wait_for_workflow() {
    local branch_name="$1"
    log_info "Waiting for GitHub Actions workflow to complete..."
    
    local max_attempts=60
    local attempt=0
    local sleep_duration=10
    
    while [ $attempt -lt $max_attempts ]; do
        # Get the latest workflow run for this branch
        local workflow_status
        workflow_status=$(gh run list --branch "$branch_name" --limit 1 --json status,conclusion --jq '.[0]')
        
        if [ -z "$workflow_status" ] || [ "$workflow_status" = "null" ]; then
            log_warning "No workflow run found yet. Waiting..."
            sleep $sleep_duration
            ((attempt++))
            continue
        fi
        
        local status
        local conclusion
        status=$(echo "$workflow_status" | jq -r '.status')
        conclusion=$(echo "$workflow_status" | jq -r '.conclusion')
        
        log_info "Workflow status: ${status}, conclusion: ${conclusion}"
        
        if [ "$status" = "completed" ]; then
            if [ "$conclusion" = "success" ]; then
                log_success "Workflow passed!"
                return 0
            else
                log_error "Workflow failed with conclusion: ${conclusion}"
                return 1
            fi
        fi
        
        sleep $sleep_duration
        ((attempt++))
    done
    
    log_error "Timeout waiting for workflow to complete"
    return 1
}

# Function to merge PR
merge_pr() {
    local pr_number="$1"
    local branch_name="$2"
    
    log_info "Preparing to merge PR #${pr_number}..."
    
    # Mark PR as ready for review (remove draft status)
    gh pr ready "$pr_number" 2>/dev/null || true
    
    # Offer to merge
    read -p "Merge PR #${pr_number}? (y/N) " -n 1 -r
    echo
    
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        log_info "Merging PR #${pr_number}..."
        
        if gh pr merge "$pr_number" --squash --delete-branch; then
            log_success "PR #${pr_number} merged successfully!"
            log_success "Branch ${branch_name} deleted."
            
            # Switch back to default branch
            local default_branch
            default_branch=$(gh repo view --json defaultBranchRef --jq '.defaultBranchRef.name')
            git checkout "$default_branch"
            git pull origin "$default_branch"
            
            return 0
        else
            log_error "Failed to merge PR #${pr_number}"
            return 1
        fi
    else
        log_info "PR merge cancelled by user."
        return 1
    fi
}

# Function to show usage
show_usage() {
    echo "Usage: $0 [OPTIONS] <issue_id>"
    echo
    echo "Options:"
    echo "  -i, --interactive    Enable interactive mode (pause between steps)"
    echo "  -h, --help          Show this help message"
    echo
    echo "Arguments:"
    echo "  issue_id            GitHub issue number to work on"
    echo
}

# Main script
main() {
    # Parse arguments
    local issue_id=""
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            -i|--interactive)
                INTERACTIVE_MODE=true
                shift
                ;;
            -h|--help)
                show_usage
                exit 0
                ;;
            -*)
                log_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
            *)
                if [ -z "$issue_id" ]; then
                    issue_id="$1"
                else
                    log_error "Multiple issue IDs provided"
                    show_usage
                    exit 1
                fi
                shift
                ;;
        esac
    done
    
    # Check if issue ID is provided
    if [ -z "$issue_id" ]; then
        log_error "Issue ID is required"
        show_usage
        exit 1
    fi
    
    # Validate issue ID is a number
    if ! [[ "$issue_id" =~ ^[0-9]+$ ]]; then
        log_error "Issue ID must be a number"
        exit 1
    fi
    
    log_info "Starting automated issue workflow for issue #${issue_id}"
    if [ "$INTERACTIVE_MODE" = true ]; then
        log_info "Interactive mode enabled - you will be prompted between steps"
    fi
    echo
    
    # Run checks
    check_requirements
    check_git_repo
    pause_if_interactive
    
    # Fetch and validate issue
    fetch_issue "$issue_id"
    echo
    pause_if_interactive
    
    # Check for uncommitted changes BEFORE creating branch
    check_clean_working_tree
    pause_if_interactive
    
    # Create branch
    local branch_name
    branch_name=$(create_branch "$issue_id")
    echo
    pause_if_interactive
    
    # Create PR
    local pr_number
    pr_number=$(create_pr "$issue_id" "$branch_name")
    echo
    pause_if_interactive
    
    # Run Copilot workflows
    log_info "Running Copilot AI workflows..."
    echo
    pause_if_interactive
    
    if ! run_copilot "/pr-implement"; then
        log_error "PR implementation failed. Please review manually."
        exit 1
    fi
    echo
    pause_if_interactive
    
    if ! run_copilot "/pr-senior-review"; then
        log_warning "Senior review had issues. Please review manually."
        exit 1
    fi
    echo
    pause_if_interactive

    if ! run_copilot "/pr-apply-review"; then
        log_warning "Applying review feedback had issues. Please review manually."
        exit 1
    fi
    echo
    pause_if_interactive
    
    # Wait for workflow to pass
    if wait_for_workflow "$branch_name"; then
        echo
        pause_if_interactive
        merge_pr "$pr_number" "$branch_name"
    else
        log_error "Workflow did not pass. Please review the PR manually at:"
        gh pr view "$pr_number" --web
        exit 1
    fi
    
    echo
    log_success "Issue #${issue_id} workflow completed successfully!"
}

# Run main function
main "$@"
