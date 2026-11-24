# Multi-stage build: the release binary is static enough to run on a slim
# image, and the console bundle is embedded in it, so the runtime stage carries
# no Node and no Rust toolchain.
#
#   docker build -t cyg-gaming .
#   docker run -p 19224:19224 -p 7977:7977/udp cyg-gaming
#
# The same file is what CI builds and what release.yml pushes to ghcr.io.

FROM node:22-bookworm AS console
WORKDIR /src/dashboard
COPY dashboard/package.json dashboard/package-lock.json ./
RUN npm ci
COPY dashboard/ ./
RUN npm run build

FROM rust:1-bookworm AS builder
WORKDIR /src

# `.cargo/config.toml` asks for mold; the base image has no ld alternative.
RUN apt-get update && apt-get install -y --no-install-recommends mold && rm -rf /var/lib/apt/lists/*

COPY . .
# The binary embeds the console at compile time (`#[folder = "../../dashboard/dist"]`).
COPY --from=console /src/dashboard/dist dashboard/dist

# Distribution build: no `-C target-cpu=native`, so the image runs on any host
# of its architecture rather than only on the machine that built it. An empty
# RUSTFLAGS would also work, but naming `generic` says why it is not native.
# `generic` is a valid LLVM CPU for both amd64 and arm64, which matters for the
# multi-arch push in release.yml.
ENV RUSTFLAGS="-C target-cpu=generic"
RUN cargo build --release --locked --bin cyg-gaming

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin openoxide

WORKDIR /app

COPY --from=builder /src/target/release/cyg-gaming /usr/local/bin/cyg-gaming
COPY game /app/game

RUN mkdir -p /app/data && chown -R openoxide:openoxide /app
USER openoxide

# 19224 TCP: HTTP API (/v1), WebSockets, dashboard, matchmaking
# 7977 UDP: WebRTC data channel
# 7778 UDP: native engine datagrams
# 7978 UDP: QUIC / WebTransport
EXPOSE 19224 7977/udp 7778/udp 7978/udp

HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:19224/status || exit 1

ENTRYPOINT ["cyg-gaming", "run", "/app/game"]
