# Compose twin of `fx-up-image-metadata-same-phase` (#467).
#
# The defect this pins is path-INDEPENDENT — it lives in the merge that folds an
# image's `devcontainer.metadata` into the configuration, and `up`'s
# single-container path, `up`'s compose path (since #448) and
# `container_metadata::resolve_config_against_container` (`exec` /
# `run-user-commands`, since #405) all share it. A single-path case would
# misattribute a regression to whichever path it happened to cover, so both
# paths are pinned and the two fixtures differ ONLY in how the container is
# brought up. See the sibling's Dockerfile for why these three phases.
#
# The label is identical to the sibling's — including its TWO entries, whose
# order the shared `onCreateCommand` pins (see that file for why one entry cannot
# express the cross-entry ordering the "Collected list" rule implies) — except
# for the marker filename, which
# is absolute here: on the compose path deacon injects no workspace mount of its
# own, so the hook's working directory is whatever the compose service provides.
# The compose file bind-mounts the fixture at `/workspace`, and naming that path
# outright keeps this case measuring the merge rather than the CWD resolution
# that `case-up-compose-lifecycle-phases` already covers.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# to the tag the COMPOSE file names — a compose fixture declares its base there,
# not in devcontainer.json.
FROM alpine:3.19
LABEL devcontainer.metadata='[{"onCreateCommand":"echo img1-onCreate >> /workspace/lifecycle.log","postCreateCommand":"echo img-postCreate >> /workspace/lifecycle.log"},{"onCreateCommand":"echo img2-onCreate >> /workspace/lifecycle.log","postStartCommand":"echo img-postStart >> /workspace/lifecycle.log"}]'
