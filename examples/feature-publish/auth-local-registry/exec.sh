#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "== Local registry auth dry-run (README: Commands) ==" >&2
if [ -z "${DEVCONTAINERS_OCI_AUTH:-}" ]; then
	echo "Warning: DEVCONTAINERS_OCI_AUTH not set; set it to registry|user|pass for real testing" >&2
fi

deacon features publish "$SCRIPT_DIR" \
	--namespace localtest/myfeatures \
	--registry localhost:5000 \
	--dry-run \
	--progress json \
	"$@"
