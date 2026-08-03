FROM alpine:3.19
# A build arg that reaches BOTH an env var and a label, so a build that dropped
# `build.args` is distinguishable from one that passed them: the default value survives
# into the image instead of the configured one.
ARG PARITY_BUILD_ARG=default-not-passed
ENV PARITY_BUILD_ARG=$PARITY_BUILD_ARG
LABEL parity.build.arg=$PARITY_BUILD_ARG
