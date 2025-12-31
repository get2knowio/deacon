#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Custom base image (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --base-image alpine:latest "$@"

echo "== Custom remote user (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --remote-user vscode "$@"

echo "== Preserve test containers (README: Commands) ==" >&2
deacon features test "$SCRIPT_DIR" --preserve-test-containers "$@"
