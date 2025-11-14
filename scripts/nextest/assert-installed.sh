#!/usr/bin/env bash
# Preflight check for cargo-nextest availability
# Exits with status 0 if installed, 1 if missing with actionable guidance

set -e

if command -v cargo-nextest >/dev/null 2>&1; then
    exit 0
fi

cat >&2 <<'EOF'
Error: cargo-nextest is not installed

cargo-nextest is required to run parallel test suites in this project.

Installation options:

  1. Via cargo (recommended):
     cargo install cargo-nextest --locked

  2. Via pre-built binaries:
     Visit https://nexte.st/book/pre-built-binaries.html

  3. Via CI action (GitHub Actions only):
     Use taiki-e/install-action@v2 in your workflow

After installation, verify with:
  cargo nextest --version

For more information:
  - Getting started: https://nexte.st/book/getting-started.html
  - Classification & usage guide: docs/testing/nextest.md
  - Quick reference: README.md (Running Tests section)

EOF

exit 1
