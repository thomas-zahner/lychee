FROM rust:trixie AS builder

WORKDIR /lychee
COPY . ./

RUN apt-get update && apt-get install -y musl-tools \
    && ARCH=$(case $(arch) in \
        "x86_64") echo "x86_64";; \
        "aarch64") echo "aarch64";; \
        *) echo "Unsupported architecture" && exit 1;; \
    esac) \
    && TARGET="$ARCH-unknown-linux-gnu" \
    && rustup target add $TARGET \
    && cargo build --release --target $TARGET \
    && strip target/$TARGET/release/lychee

# Our production image starts here, which uses
# the files from the builder image above.
FROM debian:trixie-slim

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    --no-install-recommends \
    ca-certificates \
    tzdata \
    && rm -rf /var/cache/debconf/* \
    # Clean and keep the image small. This should not
    # be necessary as the debian-slim images have an
    # auto clean mechanism but we may rely on other
    # images in the future (see:
    # https://github.com/debuerreotype/debuerreotype/blob/master/scripts/debuerreotype-minimizing-config).
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /builder/lychee /usr/local/bin/lychee
ENTRYPOINT [ "/usr/local/bin/lychee" ]
