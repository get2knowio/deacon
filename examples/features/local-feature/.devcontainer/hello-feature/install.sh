#!/bin/sh
set -e
# Feature options are exported as uppercased env vars during install:
# `greeting` -> $GREETING (default "hello" from devcontainer-feature.json).
mkdir -p /usr/local/share/local-feature
echo "${GREETING} from local feature v1.0.0" > /usr/local/share/local-feature/marker
