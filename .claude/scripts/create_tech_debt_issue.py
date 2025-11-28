#!/usr/bin/env python3
"""
Create a tech debt GitHub issue from structured input.

Usage:
    # Quick issue with minimal info
    python scripts/create_tech_debt_issue.py --title "Title" --problem "Description"

    # Issue with detailed body from file
    python scripts/create_tech_debt_issue.py --title "Title" --body-file issue.md

    # Issue with body from stdin (for Claude to pipe content)
    echo "Full markdown content..." | python scripts/create_tech_debt_issue.py --title "Title" --body -

Examples:
    # Minimal issue
    python scripts/create_tech_debt_issue.py \
        --title "Wrap ComposeManager blocking calls in spawn_blocking" \
        --problem "ComposeManager uses std::process::Command but is called from async contexts" \
        --labels compose architecture

    # Rich issue with full markdown body
    python scripts/create_tech_debt_issue.py \
        --title "Wrap ComposeManager blocking calls in spawn_blocking" \
        --body-file /tmp/issue-body.md \
        --labels compose architecture

    # Body from heredoc (useful in scripts/Claude)
    python scripts/create_tech_debt_issue.py --title "Fix blocking I/O" --labels compose --body - << 'EOF'
    ## Summary
    ComposeManager uses blocking I/O in async contexts...

    ## Problem
    The following methods use std::process::Command:
    - `start_project()` - line 150
    - `is_project_running()` - line 200

    ## Proposed Solution
    ```rust
    tokio::task::spawn_blocking(move || {
        compose_manager.start_project(&project)
    }).await?
    ```

    ## Acceptance Criteria
    - [ ] All blocking calls wrapped
    - [ ] Tests pass
    EOF

For use in maverick.fly workflow during Phase 2.5.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path


def get_existing_labels() -> set[str]:
    """Get set of existing labels in the repo."""
    try:
        result = subprocess.run(
            ['gh', 'label', 'list', '--limit', '200', '--json', 'name'],
            capture_output=True, text=True, check=True
        )
        labels_data = json.loads(result.stdout)
        return {label['name'] for label in labels_data}
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return set()


def ensure_tech_debt_label():
    """Ensure the tech-debt label exists."""
    existing = get_existing_labels()
    if 'tech-debt' not in existing:
        try:
            subprocess.run(
                ['gh', 'label', 'create', 'tech-debt',
                 '--description', 'Technical debt to be addressed',
                 '--color', 'fbca04'],
                capture_output=True, check=True
            )
        except subprocess.CalledProcessError:
            pass


TECH_DEBT_DIRECTIVES = """
---

## Implementation Directives

> **⚠️ TECH DEBT RESOLUTION REQUIREMENTS**
>
> This is a **tech debt issue** that must be resolved **in its entirety**. The following directives apply:
>
> 1. **No Partial Solutions**: Complete all acceptance criteria before closing. Do not close with "good enough" implementations.
> 2. **No Deferral**: There is no option to defer portions of this work to future issues. If scope expands during implementation, expand this issue rather than spawning follow-ups.
> 3. **No New Debt**: Do not introduce new technical debt while resolving this issue. If you encounter blocking debt, resolve it as part of this issue.
> 4. **Full Test Coverage**: All changes must include appropriate tests. Untested code is incomplete code.
> 5. **Documentation Updates**: Update any affected documentation as part of this issue.

"""


def build_issue_body(args) -> str:
    """Build issue body from arguments or provided content."""

    # If full body provided via file or stdin, append directives
    if args.body_file:
        return Path(args.body_file).read_text() + TECH_DEBT_DIRECTIVES

    if args.body:
        if args.body == '-':
            # Read from stdin
            return sys.stdin.read() + TECH_DEBT_DIRECTIVES
        else:
            return args.body + TECH_DEBT_DIRECTIVES

    # Otherwise build from structured arguments
    sections = []

    # Summary
    if args.problem:
        sections.append("## Summary\n")
        sections.append(f"{args.problem}\n")

    # Problem details
    if args.details:
        sections.append("\n## Problem\n")
        sections.append(f"{args.details}\n")

    # Why deferred
    if args.rationale:
        sections.append("\n## Why This Was Deferred\n")
        sections.append(f"{args.rationale}\n")

    # Reference pattern
    if args.pattern:
        sections.append("\n## Reference Pattern\n")
        sections.append(f"{args.pattern}\n")

    # Spec reference
    if args.spec_ref:
        sections.append("\n## Spec/Doc Reference\n")
        sections.append(f"{args.spec_ref}\n")

    # Files to modify
    if args.files:
        sections.append("\n## Files to Modify\n")
        for f in args.files:
            sections.append(f"- `{f}`\n")

    # Acceptance criteria
    if args.acceptance:
        sections.append("\n## Acceptance Criteria\n")
        criteria = args.acceptance
        if ';' in criteria:
            items = [c.strip() for c in criteria.split(';') if c.strip()]
            for item in items:
                sections.append(f"- [ ] {item}\n")
        else:
            sections.append(f"- [ ] {criteria}\n")

    # Source context
    if args.source_branch or args.source_pr:
        sections.append("\n## Context\n")
        if args.source_branch:
            sections.append(f"- Discovered while working on branch: `{args.source_branch}`\n")
        if args.source_pr:
            sections.append(f"- Related PR: #{args.source_pr}\n")

    # Append standard tech debt directives
    sections.append(TECH_DEBT_DIRECTIVES)

    return ''.join(sections)


def create_issue(args) -> str:
    """Create the GitHub issue and return its URL."""
    body = build_issue_body(args)

    # Write body to secure temp file
    with tempfile.NamedTemporaryFile(
        mode='w', delete=False, suffix='.md'
    ) as tf:
        tf.write(body)
        body_file = Path(tf.name)

    try:
        # Build labels list
        labels = ['tech-debt']
        if args.labels:
            labels.extend(args.labels)

        # Filter to existing labels
        existing = get_existing_labels()
        valid_labels = [l for l in labels if l in existing]

        # Build command
        cmd = ['gh', 'issue', 'create',
               '--title', args.title,
               '--body-file', str(body_file)]

        if valid_labels:
            cmd.extend(['--label', ','.join(valid_labels)])

        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        return result.stdout.strip()

    finally:
        if body_file.exists():
            body_file.unlink()


def main():
    parser = argparse.ArgumentParser(
        description='Create a tech debt GitHub issue',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Minimal issue
  %(prog)s --title "Fix blocking I/O" --problem "Uses std::process::Command in async"

  # Full issue
  %(prog)s \\
    --title "Wrap ComposeManager in spawn_blocking" \\
    --problem "Blocking I/O called from async context" \\
    --details "ComposeManager methods use std::process::Command..." \\
    --rationale "Pre-existing tech debt" \\
    --pattern "See docker.rs for correct pattern" \\
    --files crates/core/src/compose.rs crates/deacon/src/commands/up.rs \\
    --acceptance "All compose calls wrapped in spawn_blocking" \\
    --labels compose architecture \\
    --source-branch 005-compose-mount-env
        """
    )

    # Required
    parser.add_argument('--title', required=True,
                        help='Issue title')

    # Body options (mutually exclusive approaches)
    body_group = parser.add_mutually_exclusive_group()
    body_group.add_argument('--body', metavar='TEXT_OR_DASH',
                            help='Full markdown body (use "-" to read from stdin)')
    body_group.add_argument('--body-file', metavar='FILE',
                            help='Read body from file')
    parser.add_argument('--problem',
                        help='Brief problem description (used if no --body/--body-file)')

    # Optional enrichment (used if no --body/--body-file)
    parser.add_argument('--details',
                        help='Detailed problem explanation')
    parser.add_argument('--rationale',
                        help='Why this was deferred')
    parser.add_argument('--pattern',
                        help='Reference pattern to follow')
    parser.add_argument('--spec-ref',
                        help='Spec or documentation reference')
    parser.add_argument('--files', nargs='+',
                        help='Files that need to be modified')
    parser.add_argument('--acceptance',
                        help='Acceptance criteria (semicolon-separated for multiple)')
    parser.add_argument('--labels', nargs='+',
                        help='Additional labels (tech-debt is always added)')

    # Context
    parser.add_argument('--source-branch',
                        help='Branch where this was discovered')
    parser.add_argument('--source-pr', type=int,
                        help='PR number where this was discovered')

    # Modes
    parser.add_argument('--dry-run', action='store_true',
                        help='Print issue body without creating')
    parser.add_argument('--json', action='store_true',
                        help='Output JSON with issue URL')

    args = parser.parse_args()

    # Validate: need either body content or at least a problem description
    if not args.body and not args.body_file and not args.problem:
        parser.error("Must provide either --body, --body-file, or --problem")

    if args.dry_run:
        print("=== Issue Title ===")
        print(args.title)
        print("\n=== Issue Body ===")
        print(build_issue_body(args))
        return

    # Ensure label exists
    ensure_tech_debt_label()

    # Create issue
    try:
        url = create_issue(args)

        if args.json:
            print(json.dumps({'url': url, 'title': args.title}))
        else:
            print(url)

    except subprocess.CalledProcessError as e:
        error = {'error': e.stderr or str(e)}
        if args.json:
            print(json.dumps(error))
        else:
            print(f"Error: {error['error']}", file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    main()
