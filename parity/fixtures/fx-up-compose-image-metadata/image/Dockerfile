# Fixture image for the compose-path image-metadata case (#448).
#
# The hook and `META_ENV` live ONLY in this image's `devcontainer.metadata`
# label — the sibling devcontainer.json declares no hook at all and contributes
# `WS_ENV` through `remoteEnv` alone. So the marker file can only appear, and
# can only carry `from-image-metadata from-workspace-config`, if the CLI folded
# the SERVICE IMAGE's metadata into the configuration it ran the compose
# project's post-create hook with.
#
# NO `remoteUser` here, deliberately — and that omission is the whole reason
# this case is stable. An image-metadata `remoteUser` DOES reach the hook after
# #448 (measured: both CLIs report `"remoteUser":"metauser"` on this compose
# project), but a NON-ROOT hook then cannot write into the bind-mounted
# workspace unless its uid matches the host directory's owner. The reference
# applies `updateRemoteUserUID` on the compose path and remaps the user's uid to
# the host's; deacon applies that mapping on the single-container path only, so
# its hook keeps the image's uid. On a host whose uid happens to match the image
# user's the difference is invisible; on GitHub's runners it is `Permission
# denied`. That is a REAL and separate deacon gap — filed as #462, NOT masked
# here — and folding it into this case would make a #448 regression
# indistinguishable from a uid coincidence. The `remoteUser` cell of #448's
# matrix is measured instead by the Docker-gated
# `integration_compose_config_mounts::test_compose_up_merges_service_image_metadata`,
# which owns its workspace directory and makes it world-writable so permissions
# are not part of what it measures.
#
# The hook is the STRING form, also deliberately: deacon's compose post-create
# runs `postCreateCommand` only when it is a string (the array and object forms,
# and every phase other than postCreate, are a separate pre-existing gap on this
# path — #460). Using the string form keeps this case measuring the
# image-metadata merge and nothing else.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the tag the fixture's COMPOSE
# file names — a compose fixture declares its base there, not in
# devcontainer.json. Both CLIs resolve the label by local `docker inspect`, so
# no registry is involved.
FROM alpine:3.19
LABEL devcontainer.metadata='[{"remoteEnv":{"META_ENV":"from-image-metadata"},"postCreateCommand":"echo ${META_ENV:-UNSET} ${WS_ENV:-UNSET} > /workspace/up-compose-image-metadata.txt"}]'
