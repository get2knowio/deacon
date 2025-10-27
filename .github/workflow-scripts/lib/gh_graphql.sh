#!/usr/bin/env bash
set -euo pipefail

# Helper script for GitHub GraphQL API calls.
# Requires GH_TOKEN environment variable to be set.
if [[ -z "${GH_TOKEN:-}" ]]; then
  echo "::error::GH_TOKEN not set" >&2
  exit 1
fi

# Forward arguments to gh api graphql with the Projects Next header.
gh api graphql -H "GraphQL-Features: projects_next_graphql" "$@"
