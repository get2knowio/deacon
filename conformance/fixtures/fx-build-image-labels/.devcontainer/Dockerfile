FROM alpine:3.19
# The image-level facts a `build` must actually put in the image it produces: an
# authored label, an authored environment variable, and a working directory. Observing
# these on the built image is the only way to tell a build that produced the RIGHT image
# from one that merely exited zero.
LABEL parity.build.token=conformance-image-labels
ENV PARITY_BUILD_MARKER=authored-in-dockerfile
WORKDIR /parity
