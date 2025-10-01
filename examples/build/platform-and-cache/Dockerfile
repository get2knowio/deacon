FROM alpine:3.19

LABEL example.type=platform-and-cache

RUN echo "Building for platform: $(uname -m)" && \
    echo "This build demonstrates platform and cache options"

# Add some files to make caching effects visible
RUN date > /build-timestamp.txt && \
    echo "Build completed at: $(date)" && \
    cat /build-timestamp.txt
