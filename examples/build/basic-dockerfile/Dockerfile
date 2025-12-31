FROM alpine:3.19

# Build argument that can be passed during build
ARG FOO=default_value

# Set environment variable from build arg
ENV FOO_ENV=$FOO

# Add a unique label for validation
LABEL parity.token=__UUID__
LABEL example.type=basic-dockerfile
LABEL build.arg.foo=$FOO

# Simple test to show the container works
RUN echo "Building with FOO=$FOO"
