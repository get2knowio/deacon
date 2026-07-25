#!/usr/bin/env bash
# Warm the local image cache for every pinned image the declarative conformance
# fixtures name, BEFORE the live parity lane runs.
#
# Why this exists: `read-configuration --include-merged-configuration` reads an image's
# `devcontainer.metadata` LABEL and pulls on a cache miss, matching the reference CLI
# (#307). Some fixture bases are large — `mcr.microsoft.com/devcontainers/universal:2-linux`
# is 9.93 GB — and a first pull takes minutes while producing no HTTP progress the harness
# can log. The 120s per-invocation bound then fires and reads as a HANG rather than as
# first-pull latency, which is how it was misdiagnosed once already (023 T116).
#
# Warming the cache up front means the bound guards real hangs, not downloads. The parity
# workflow does the same thing in its own step; this is the local equivalent so a
# developer's `make test-parity` behaves like CI.
#
# Failures are non-fatal: an unreachable registry should surface as the parity run's own
# cause-specific error, not as an opaque pre-step failure.
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixtures="${root}/conformance/fixtures"

if [ ! -d "${fixtures}" ]; then
  echo "prepull: no ${fixtures} directory; nothing to warm" >&2
  exit 0
fi

# A `:local` tag names an image no registry provides — e.g. the object-form
# `devcontainer.metadata` label (#300/#332), which every published image dropped in favor
# of the array form. Build it from its sibling Dockerfile so both CLIs resolve the label by
# local `docker inspect`. This mirrors the parity workflow's "Build declarative-fixture
# local images" step, and must run BEFORE the pull loop skips `:local` tags — otherwise a
# developer's `make test-parity` fails on a missing image where CI passes.
for df in "${fixtures}"/*/image/Dockerfile; do
  [ -e "${df}" ] || continue
  dir="$(dirname "${df}")"
  cfg="$(dirname "${dir}")/.devcontainer/devcontainer.json"
  [ -e "${cfg}" ] || continue
  tag="$(sed -nE 's/.*"image"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' "${cfg}" | head -n1)"
  [ -n "${tag}" ] || continue
  echo "prepull: docker build ${tag}"
  docker build -q -t "${tag}" "${dir}" >/dev/null \
    || echo "prepull: WARN could not build ${tag} (the parity run will report its own cause)" >&2
done

images="$(grep -rhoE '"image"[[:space:]]*:[[:space:]]*"[^"]+"' \
            "${fixtures}"/*/.devcontainer/devcontainer.json 2>/dev/null \
          | sed -E 's/.*"image"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' \
          | sort -u \
          | grep -v ':local$' || true)"

if [ -z "${images}" ]; then
  echo "prepull: no pinned fixture images found under ${fixtures}" >&2
  exit 0
fi

while read -r img; do
  [ -n "${img}" ] || continue
  if docker image inspect "${img}" >/dev/null 2>&1; then
    echo "prepull: already cached ${img}"
    continue
  fi
  echo "prepull: docker pull ${img}"
  docker pull -q "${img}" >/dev/null 2>&1 \
    || echo "prepull: WARN could not pull ${img} (the parity run will report its own cause)" >&2
done <<< "${images}"
