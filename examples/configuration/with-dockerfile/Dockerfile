# syntax=docker/dockerfile:1
ARG VARIANT="1.21"
FROM mcr.microsoft.com/devcontainers/javascript-node:${VARIANT}

WORKDIR /workspace
COPY package.json package-lock.json* ./
RUN npm ci || true
COPY . .
