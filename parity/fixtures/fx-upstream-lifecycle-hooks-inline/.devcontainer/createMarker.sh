#!/bin/bash
# Adapted from devcontainers/cli v0.87.0
# src/test/container-features/configs/lifecycle-hooks-inline-commands/.devcontainer/createMarker.sh
#
# Upstream names each marker `<counter>.<name>` and dumps `printenv` into it, which pins
# order but makes the file's CONTENT a per-host environment snapshot — unusable as a
# compared observable. This records the same fact (which hook ran, and in what order)
# as one deterministic append-only log.
MARKER_FILE_NAME="$1"
echo "${MARKER_FILE_NAME}" >> lifecycle-order.log
