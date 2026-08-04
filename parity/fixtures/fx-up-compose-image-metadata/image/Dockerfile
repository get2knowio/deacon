# Fixture image for the compose-path image-metadata case (#448, #462).
#
# `remoteUser`, the hook and `META_ENV` live ONLY in this image's
# `devcontainer.metadata` label — the sibling devcontainer.json declares none of
# them and contributes `WS_ENV` through `remoteEnv` alone. So the marker file can
# only appear, and can only carry `metauser from-image-metadata
# from-workspace-config`, if the CLI folded the SERVICE IMAGE's metadata into the
# configuration it ran the compose project's post-create hook with. One line,
# all four cells of #448's matrix: the hook ran, it ran as the image's
# `remoteUser`, the image's `remoteEnv` reached it, and the workspace's did too.
#
# `metauser` is pinned to uid 1234 rather than letting `adduser` pick the first
# free uid (1000, the commonest host uid there is), and that pin is what makes
# the case load-bearing for #462. The workspace is a bind mount of a host
# directory the runner owns, so a NON-ROOT hook can only write into it once its
# uid has been remapped to the HOST's — `updateRemoteUserUID`, which both CLIs
# apply on the compose path as of #462 (the reference by building a derived
# `<image>-uid` image and pointing the service at it through its generated
# override; deacon by remapping in the running container through the same
# `apply_user_mapping` its single-container path has always used). Pinning 1234
# makes the mismatch the NORM here rather than an accident: if the remap
# regresses, the hook dies with `Permission denied` and this case fails on every
# host instead of only on hosts whose uid happens to disagree with the image's.
#
# This is why the fixture's first revision declared `remoteUser` and its second
# removed it: before #462 the case passed locally purely because the dev
# container's uid and `adduser`'s first free uid were both 1000, and failed on
# GitHub's runners. With the remap in place the uid coincidence is gone from the
# verdict entirely, so the `remoteUser` cell lives here again.
#
# The hook is the STRING form, also deliberately: deacon's compose post-create
# runs `postCreateCommand` only when it is a string (the array and object forms,
# and every phase other than postCreate, are a separate pre-existing gap on this
# path — #460). Using the string form keeps this case measuring the
# image-metadata merge and the uid remap, and nothing else.
#
# The marker records `whoami`, not `id -u`: the remapped uid is the HOST's and so
# differs between a dev box and a CI runner, which would make a fixed assertion a
# statement about the host. The uid is what makes the WRITE possible, and the
# write is what the case observes — so the remap is measured by the file
# existing at all, and the user by its name.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the tag the fixture's COMPOSE
# file names — a compose fixture declares its base there, not in
# devcontainer.json. Both CLIs resolve the label by local `docker inspect`, so
# no registry is involved.
FROM alpine:3.19
RUN adduser -D -u 1234 -s /bin/sh metauser
LABEL devcontainer.metadata='[{"remoteUser":"metauser","remoteEnv":{"META_ENV":"from-image-metadata"},"postCreateCommand":"echo $(whoami) ${META_ENV:-UNSET} ${WS_ENV:-UNSET} > /workspace/up-compose-image-metadata.txt"}]'
