FROM alpine:3.19
ENV CONF_FROM_IMAGE=image
RUN mkdir -p /opt/conformance/bin && printf '#!/bin/sh\necho conformance-tool\n' > /opt/conformance/bin/conf-tool && chmod +x /opt/conformance/bin/conf-tool
ENV PATH=/opt/conformance/bin:$PATH
