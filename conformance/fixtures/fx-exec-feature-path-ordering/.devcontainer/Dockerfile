FROM alpine:3.19
RUN mkdir -p /opt/image/bin \
 && printf '#!/bin/sh\necho image-tool\n' > /opt/image/bin/image-tool \
 && chmod +x /opt/image/bin/image-tool
ENV PATH=/opt/image/bin:$PATH
