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
fixtures="${root}/parity/fixtures"

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
# Locate every fixture config by SEARCH, never by a fixed relative path. A fixture's
# config may sit at `<fx>/.devcontainer/devcontainer.json` or at a nested
# `<fx>/<subdir>/devcontainer.jsonc` (both shapes exist — `fx-readconfig-basic` uses the
# latter), and the `.jsonc` extension is as valid as `.json`. The earlier
# `*/.devcontainer/devcontainer.json` glob silently matched neither, which is the same
# "a glob that matches nothing is not an error" failure as 023 T116 — the bug this whole
# script was written to prevent, reintroduced one directory level down.
configs="$(find "${fixtures}" \
             \( -name 'devcontainer.json' -o -name 'devcontainer.jsonc' \) \
             -type f 2>/dev/null | sort)"

if [ -z "${configs}" ]; then
  echo "prepull: FATAL no devcontainer config found under ${fixtures} — the fixture" >&2
  echo "prepull: discovery is broken, and warming nothing would read as a hang later" >&2
  exit 1
fi

# Read an image reference out of a config (first `"image": "..."` wins).
config_image() {
  sed -nE 's/.*"image"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' "$1" | head -n1
}

# A compose fixture names its base in the COMPOSE file, not in devcontainer.json — that
# config carries `dockerComposeFile` + `service` and no `image` key at all. Warming only
# the JSON-declared images therefore left every compose base cold, which is the same
# "discovery that matches nothing is not an error" failure this script's header describes,
# one file format down: `fx-tier1-compose-array` pulls
# `mcr.microsoft.com/devcontainers/base:bookworm` on the merged-configuration read, blows
# the harness's 120s bound, and the runner reports a HANG naming the case rather than the
# download. Because that abort is fail-loud, it also takes the whole run with it — no other
# case gets to report.
#
# ALL images are read, not the first: a compose project legitimately has several services
# (`fx-tier1-compose-postgres` has an app and a database), and warming one of them is the
# same cold-cache stall on the other. `${VAR}` interpolation is skipped — it is not a
# resolvable reference here, and a fixture that needed one would be unpinned (V18).
compose_images() {
  sed -nE 's/^[[:space:]]+image:[[:space:]]*"?([^"[:space:]#]+)"?.*/\1/p' "$1" \
    | grep -v '\$' || true
}

for df in "${fixtures}"/*/image/Dockerfile; do
  [ -e "${df}" ] || continue
  dir="$(dirname "${df}")"
  fx="$(dirname "${dir}")"
  # The sibling config, found the same way — not assumed at a fixed path.
  cfg="$(printf '%s\n' "${configs}" | grep "^${fx}/" | head -n1)"
  [ -n "${cfg}" ] || continue
  tag="$(config_image "${cfg}")"
  if [ -z "${tag}" ]; then
    # A COMPOSE fixture with a local base names it in the compose file, not in
    # devcontainer.json — that config carries `dockerComposeFile` + `service` and no
    # `image` key, so `config_image` finds nothing and the build was skipped silently.
    # The `:local` tag is the one no registry provides, which is exactly the tag this
    # loop exists to produce; a compose fixture whose services are all published images
    # still resolves to nothing here and is left to the pull loop below.
    # (`fx-up-compose-image-metadata`, #448.)
    tag="$(find "${fx}" \( -name '*.yml' -o -name '*.yaml' \) -type f 2>/dev/null \
             | sort \
             | while read -r yml; do compose_images "${yml}"; done \
             | grep ':local$' | head -n1)"
  fi
  [ -n "${tag}" ] || continue
  echo "prepull: docker build ${tag}"
  docker build -q -t "${tag}" "${dir}" >/dev/null \
    || echo "prepull: WARN could not build ${tag} (the parity run will report its own cause)" >&2
done

config_images="$(printf '%s\n' "${configs}" \
                 | while read -r cfg; do config_image "${cfg}"; done)"

if [ -z "${config_images}" ]; then
  echo "prepull: FATAL no image reference found in any of these configs:" >&2
  printf '%s\n' "${configs}" >&2
  exit 1
fi

# Compose files are discovered the same way the configs are — by search at any depth, both
# extensions — never by a fixed relative path. Finding none is NOT fatal: a fixture tree
# with no compose fixture is legitimate, unlike a tree with no devcontainer config at all.
compose_files="$(find "${fixtures}" \
                   \( -name '*.yml' -o -name '*.yaml' \) \
                   -type f 2>/dev/null | sort)"

compose_declared="$(printf '%s\n' "${compose_files}" \
                    | while read -r yml; do
                        [ -n "${yml}" ] || continue
                        compose_images "${yml}"
                      done)"

images="$(printf '%s\n%s\n' "${config_images}" "${compose_declared}" \
          | sed '/^$/d' \
          | sort -u \
          | grep -v ':local$' || true)"

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
