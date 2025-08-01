FROM alpine:latest AS builder
WORKDIR /builder

ARG LYCHEE_VERSION="latest"

RUN apk add --no-cache ca-certificates jq wget \
    && ARCH=$(case $(arch) in \
        "x86_64") echo "x86_64";; \
        "aarch64") echo "aarch64";; \
        *) echo "Unsupported architecture" && exit 1;; \
        esac) \
    && BASE_URL=$(case $LYCHEE_VERSION in \
        "latest" | "nightly") echo "https://github.com/lycheeverse/lychee/releases/latest/download";; \
        *) echo "https://github.com/lycheeverse/lychee/releases/download/$LYCHEE_VERSION";; \
        esac) \
    && wget -q -O - "$BASE_URL/lychee-$ARCH-unknown-linux-musl.tar.gz" | tar -xz lychee \
    && chmod +x lychee

FROM alpine:latest
RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S lychee \
    && adduser -D -G lychee -S lychee

COPY --from=builder /builder/lychee /usr/local/bin/lychee
# Run as non-root user
USER lychee
ENTRYPOINT [ "/usr/local/bin/lychee" ]
