ARG BASE_TAG=bookworm
FROM debian:${BASE_TAG}-slim

ARG FROM_LOCAL_ENV=unknown
RUN echo "from_local_env=${FROM_LOCAL_ENV}" > /etc/built-args
