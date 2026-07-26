FROM alpine:3.19
# Deliberate build-time failure: the command exits non-zero, so the image is never produced.
RUN echo "conformance: failing on purpose" >&2 && exit 17
