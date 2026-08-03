# Fixture image for the compose-path image-metadata case (#448).
#
# Every lifecycle-relevant property here lives ONLY in the image's
# `devcontainer.metadata` label — the sibling devcontainer.json declares no
# `remoteUser` and no hook, and contributes `WS_ENV` through `remoteEnv` alone.
# So the marker file can only appear, and can only carry
# `metauser from-image-metadata from-workspace-config`, if the CLI folded the
# SERVICE IMAGE's metadata into the configuration it ran the compose project's
# post-create hook with.
#
# The hook is the STRING form deliberately: deacon's compose post-create runs
# `postCreateCommand` only when it is a string (the array and object forms, and
# every phase other than postCreate, are a separate pre-existing gap on this
# path — see the case notes). Using the string form keeps this case measuring
# the image-metadata merge and nothing else.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the tag the fixture's COMPOSE
# file names — a compose fixture declares its base there, not in
# devcontainer.json. Both CLIs resolve the label by local `docker inspect`, so
# no registry is involved.
FROM alpine:3.19
RUN adduser -D -s /bin/sh metauser
LABEL devcontainer.metadata='[{"remoteUser":"metauser","remoteEnv":{"META_ENV":"from-image-metadata"},"postCreateCommand":"echo $(whoami) ${META_ENV:-UNSET} ${WS_ENV:-UNSET} > /workspace/up-compose-image-metadata.txt"}]'
