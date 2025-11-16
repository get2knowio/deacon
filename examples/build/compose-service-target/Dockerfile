FROM alpine:3.19

WORKDIR /workspace

# Install basic development tools
RUN apk add --no-cache git bash

# Create workspace directory
RUN mkdir -p /workspace

LABEL example.type="compose-service-target"
