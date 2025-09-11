FROM ubuntu:20.04

# Set workspace directory
WORKDIR ${localWorkspaceFolder}

# Install basic tools
RUN apt-get update && apt-get install -y \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace
COPY ${localWorkspaceFolder} ${localWorkspaceFolder}