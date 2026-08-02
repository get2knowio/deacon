FROM alpine:3.19 AS base
# The only stage this Dockerfile defines is `base`. `build.target` names another one,
# which nothing can discover until the builder resolves the graph.
RUN echo conformance > /base-stage
