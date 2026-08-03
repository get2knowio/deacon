FROM alpine:3.19
# Deliberately plain. What this fixture exists to observe is the metadata the
# CONFIG contributes (`remoteUser`, `containerEnv`) — the entries a `--output`
# build must record on the image it exports, and once recorded none at all.
LABEL parity.build.token=conformance-build-output-metadata
