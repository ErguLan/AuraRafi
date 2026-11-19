# syntax=docker/dockerfile:1

# Build stage. Workspace manifests and sources are copied explicitly so the
# repository-local .cargo/config.toml (Windows target) never enters the image.
FROM rust:1.83-bookworm AS builder

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libudev-dev \
        libxkbcommon-dev \
        libwayland-dev \
        libx11-xcb-dev \
        libxcb1-dev \
        libxrandr-dev \
        libxi-dev \
        libxcursor-dev \
        libfontconfig1-dev \
        cmake \
        clang \
        lld \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-gnu

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY assets ./assets
COPY crates ./crates
COPY editor ./editor

ENV CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu \
    CARGO_TARGET_DIR=/build/target \
    CARGO_INCREMENTAL=0

RUN cargo build --release -p raf_player --bin aurarafi-player

# Runtime stage. Vulkan is the normal headless executor; the player keeps its
# CPU recovery path when no adapter is available.
FROM debian:bookworm-slim AS runtime

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        mesa-vulkan-drivers \
        libvulkan1 \
        libudev1 \
        libxkbcommon0 \
        libxkbcommon-x11-0 \
        libwayland-client0 \
        libx11-6 \
        libx11-xcb1 \
        libxcb1 \
        libxrandr2 \
        libxi6 \
        libxcursor1 \
        libfontconfig1 \
        ca-certificates \
        tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 10001 rafi \
    && useradd --uid 10001 --gid 10001 --home-dir /home/rafi \
        --create-home --shell /usr/sbin/nologin rafi \
    && install -d -o rafi -g rafi -m 0755 /session /session/project /session/out

# The compiled player is kept under an explicit binary name; the published
# entrypoint at /usr/local/bin/aurarafi-player is deploy/entrypoint.sh, which
# validates the manifest and then execs this binary.
COPY --from=builder /build/target/x86_64-unknown-linux-gnu/release/aurarafi-player /usr/local/bin/aurarafi-player-bin
COPY deploy/entrypoint.sh /usr/local/bin/aurarafi-player
# Normalize line endings so a Windows checkout cannot break the shebang, then
# publish both files as executable.
RUN sed -i 's/\r$//' /usr/local/bin/aurarafi-player \
    && chmod 0755 /usr/local/bin/aurarafi-player-bin /usr/local/bin/aurarafi-player

ENV HOME=/home/rafi \
    RAFI_HEADLESS=1 \
    RAFI_MAX_FRAMES=0 \
    RAFI_IDLE_TIMEOUT_SECONDS=60

WORKDIR /session
USER rafi

# /session/project is the read-only project mount; /session/out is the only
# writable destination. The token and manifest stay outside image layers.
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/aurarafi-player"]
CMD ["--headless"]
