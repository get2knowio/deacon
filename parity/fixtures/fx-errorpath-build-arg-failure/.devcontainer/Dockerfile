FROM alpine:3.19
# Fails ONLY when the configuration's build argument reaches the build. A CLI that
# dropped `build.args` would succeed here, which is what makes the failure evidence.
ARG CONF_FAIL=0
RUN if [ "$CONF_FAIL" = "1" ]; then echo "conformance: build arg reached the build" >&2; exit 19; fi
