#  Copyright (c) Microsoft Corporation. All rights reserved.
#  Licensed under the MIT License. See License.txt in the project root for license information.
ARG VARIANT="bookworm-slim"

# Target should skip this layer
FROM alpine:3.19 AS false-start

FROM debian:${VARIANT} AS desired-image

RUN echo "||test-content||" > /var/test-marker

# Target should skip this layer
FROM alpine:3.19 AS false-finish
