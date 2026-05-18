FROM rust:trixie AS builder

WORKDIR /lychee
COPY . ./

RUN apt-get update && apt-get install -y musl-tools \
    && ARCH=$(case $(arch) in \
        "x86_64") echo "x86_64";; \
        "aarch64") echo "aarch64";; \
        *) echo "Unsupported architecture" && exit 1;; \
    esac) \
    && TARGET="$ARCH-unknown-linux-musl" \
    && rustup target add $TARGET \
    && cargo build --release --target $TARGET \
    && strip target/$TARGET/release/lychee

# Our production image starts here, which uses
# the files from the builder image above.
FROM alpine:latest
COPY --from=builder /lychee/target/x86_64-unknown-linux-musl/release/lychee /usr/local/bin/lychee

RUN ls -l /usr/local/bin/lychee

RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S lychee \
    && adduser -D -G lychee -S lychee

# Run as non-root user
USER lychee
ENTRYPOINT [ "/usr/local/bin/lychee" ]
