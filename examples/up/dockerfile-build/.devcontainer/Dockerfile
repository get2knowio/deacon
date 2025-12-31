FROM node:20-alpine

# Install common development tools
RUN apk add --no-cache \
    git \
    openssh-client \
    bash \
    sudo

# Create devuser without specifying UID/GID to avoid conflicts
RUN adduser -D -s /bin/bash devuser \
    && echo "devuser ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/devuser \
    && chmod 0440 /etc/sudoers.d/devuser

# Copy test file from build context (parent directory) to verify context works
COPY test-context-file.txt /tmp/context-test.txt

# Set working directory
WORKDIR /workspace
