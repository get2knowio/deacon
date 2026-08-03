# Fixture image for the `run-user-commands` image-metadata cases (#405).
#
# Every lifecycle-relevant property here lives ONLY in the image's
# `devcontainer.metadata` label — the sibling devcontainer.json declares no
# `remoteUser`, no `remoteEnv` and no hook at all. So the marker file can only
# appear, and can only carry `metauser from-image-metadata`, if the CLI folded
# the image metadata into the configuration it ran the user commands with.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the tag the fixture's
# devcontainer.json names. Both CLIs resolve the label by local `docker
# inspect`, so no registry is involved.
FROM alpine:3.19
RUN adduser -D -s /bin/sh metauser
LABEL devcontainer.metadata='[{"remoteUser":"metauser","remoteEnv":{"META_ENV":"from-image-metadata"},"postCreateCommand":["sh","-c","echo $(whoami) ${META_ENV:-UNSET} > /workspace/ruc-image-metadata.txt"]}]'
