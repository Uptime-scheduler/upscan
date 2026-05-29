FROM --platform=linux/amd64 alpine:3.20

ARG VERSION=0.1.0

# The musl binary is statically linked - no extra deps needed
RUN apk add --no-cache curl tar

RUN curl -fLo /tmp/upscan.tar.gz \
    "https://github.com/Uptime-scheduler/upscan/releases/download/v${VERSION}/upscan-${VERSION}-x86_64-unknown-linux-musl.tar.gz" \
    && tar -xzf /tmp/upscan.tar.gz -C /tmp/ \
    && find /tmp -maxdepth 2 -name upscan -type f -exec mv {} /usr/local/bin/upscan \; \
    && rm /tmp/upscan.tar.gz \
    && chmod +x /usr/local/bin/upscan

COPY smoke_test.sh /smoke_test.sh
RUN chmod +x /smoke_test.sh

CMD ["/smoke_test.sh"]
