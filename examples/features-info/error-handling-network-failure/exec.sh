#!/usr/bin/env bash
set -euo pipefail

echo "== Simulated timeout (README: Running - simulated timeout) ==" >&2
set +e
deacon features info manifest registry.example.invalid/feature:1 "$@"
echo "Exit code (expected 1): $?" >&2

echo "== DNS resolution failure ==" >&2
deacon features info manifest nonexistent.registry.example/feature:1 "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Connection refused ==" >&2
deacon features info manifest localhost:9999/feature:1 "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Timeout with debug logging ==" >&2
deacon features info manifest registry.example.invalid/feature:1 --log-level debug "$@"
echo "Exit code (expected 1): $?" >&2

echo "== JSON mode outputs empty object on error ==" >&2
deacon features info manifest registry.example.invalid/feature:1 --output-format json "$@"
echo "Exit code (expected 1): $?" >&2
set -e
