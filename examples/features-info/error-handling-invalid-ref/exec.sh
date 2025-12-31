#!/usr/bin/env bash
set -euo pipefail

echo "== Invalid reference (text) ==" >&2
set +e
deacon features info manifest invalid-feature-ref "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Non-existent feature (text) ==" >&2
deacon features info manifest ghcr.io/does-not-exist/fake-feature:1.0.0 "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Malformed registry URL (text) ==" >&2
deacon features info manifest not.a.registry//feature "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Invalid reference (json) ==" >&2
deacon features info manifest invalid-feature-ref --output-format json "$@"
echo "Exit code (expected 1): $?" >&2

echo "== Non-existent feature (json) ==" >&2
deacon features info manifest ghcr.io/does-not-exist/fake-feature:1.0.0 --output-format json "$@"
echo "Exit code (expected 1): $?" >&2
set -e
