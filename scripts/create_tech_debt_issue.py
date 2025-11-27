#!/usr/bin/env python3
"""
Create a tech debt GitHub issue from structured input.

Usage:
    python scripts/create_tech_debt_issue.py --title "Title" --problem "Description" [options]

Example:
    python scripts/create_tech_debt_issue.py \
        --title "Wrap ComposeManager blocking calls in spawn_blocking" \
        --problem "ComposeManager uses std::process::Command but is called from async contexts" \
        --rationale "Pre-existing tech debt, not specific to this feature" \
        --pattern "See docker.rs spawn_blocking usage" \
        --files "crates/core/src/compose.rs" \
        --spec-ref "CLAUDE.md async safety guidelines" \
        --labels "compose,architecture"

The script will:
1. Use Claude (via the conversation context) to expand the minimal input into a detailed issue
2. Create the GitHub issue with appropriate labels
3. Output the issue URL

For use in maverick.fly workflow during Phase 2.5.
"""

import argparse
import json
import subprocess
import sys
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


def build_issue_body(args) -> str:
    """Build a structured issue body from the arguments."""
    sections = []

    # Summary
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

    return ''.join(sections)


def create_issue(args) -> str:
    """Create the GitHub issue and return its URL."""
    body = build_issue_body(args)

    # Write body to temp file
    body_file = Path('/tmp/tech_debt_issue.md')
    body_file.write_text(body)

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
    parser.add_argument('--problem', required=True,
                        help='Brief problem description')

    # Optional enrichment
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
