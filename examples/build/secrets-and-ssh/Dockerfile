FROM alpine:3.19

LABEL example.type=secrets-and-ssh

# Install git for SSH demonstration
RUN apk add --no-cache git openssh-client

# Demonstrate secret mount
# Note: --mount=type=secret requires BuildKit
RUN --mount=type=secret,id=foo \
    if [ -f /run/secrets/foo ]; then \
        echo "Secret 'foo' is available"; \
        echo "Secret length: $(wc -c < /run/secrets/foo) bytes"; \
    else \
        echo "Secret 'foo' not provided (this is expected in some scenarios)"; \
    fi

# Demonstrate SSH forwarding
# Note: --mount=type=ssh requires BuildKit
RUN --mount=type=ssh \
    if [ -S "$SSH_AUTH_SOCK" ]; then \
        echo "SSH agent socket available at: $SSH_AUTH_SOCK"; \
        ssh-add -l 2>/dev/null || echo "No SSH keys loaded (this is expected without SSH agent)"; \
    else \
        echo "SSH agent not available"; \
    fi

# Create a marker file to show the build completed
RUN echo "Build completed with BuildKit features" > /buildkit-test.txt
