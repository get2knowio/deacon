FROM node:20-alpine

# Create a non-root user
RUN addgroup -g 1000 devuser && \
    adduser -D -u 1000 -G devuser devuser

# Install common development tools
RUN apk add --no-cache \
    git \
    openssh-client \
    bash

# Set working directory
WORKDIR /workspace

# Switch to non-root user
USER devuser
