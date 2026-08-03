ARG VARIANT=bookworm
FROM mcr.microsoft.com/devcontainers/base:${VARIANT} AS base

ARG NODE_VERSION=20
RUN echo "node ${NODE_VERSION}" > /etc/node-version

FROM base AS dev
RUN echo "dev stage" > /etc/stage
