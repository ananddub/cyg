# CYG server image — one game project at /app/game, server binary self-contained.
# Runtime env: DATABASE_URL (postgres), OX_ALLOW_DEFAULT_SECRETS=1 (dev).
# Example:
#   docker run -p 19224:19224 -p 7977:7977/udp -p 7978:7978/udp -p 7778:7778/udp \
#     -e DATABASE_URL=postgres://... dubeyanand/cyg-gaming:v0.1.0

FROM fedora:latest

RUN dnf install -y ca-certificates curl && dnf clean all \
    && useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin openoxide

WORKDIR /app

COPY target/release/cyg-gaming /usr/local/bin/cyg-gaming
COPY game /app/game

RUN chown -R openoxide:openoxide /app
USER openoxide

# 7777 TCP: game WebSocket + matchmake + static website
# 8090 TCP: secondary website/WS listener
# 7778 UDP: native engine datagrams
# 7779 UDP: WebRTC data/voice
EXPOSE 7777 8090 7778/udp 7779/udp

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:7777/health || exit 1

ENTRYPOINT ["cyg-gaming", "run", "/app/game"]
