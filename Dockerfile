# Flash, self-hosted: the open server on a slim Debian base.
#
#   docker build -t flash .
#   docker run -d -p 127.0.0.1:8437:8437 -v flash-data:/data \
#       -e FLASH_BASE_URL=https://cards.example.com flash
#
# The binary embeds its templates and assets; /data holds the SQLite
# database, media and import scratch space. The first boot logs the
# admin enrollment link.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p flash-server \
    && install -m 0755 target/release/flash-server /flash-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    # The ids are pinned to the ones the 0.1.0 image (Alpine) gave its
    # `flash` user. Files in a volume keep their numeric owner, so a
    # server with any other uid cannot open the database 0.1.0 wrote.
    && groupadd --system --gid 101 flash \
    && useradd --system --uid 100 --gid flash --home-dir /data flash \
    && install -d -o flash -g flash /data
COPY --from=build /flash-server /usr/local/bin/flash-server
USER flash
ENV FLASH_DATA_DIR=/data \
    FLASH_BIND=0.0.0.0:8437
VOLUME /data
EXPOSE 8437
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
    CMD wget -qO- http://127.0.0.1:8437/healthz | grep -q '"db":"ok"' || exit 1
ENTRYPOINT ["/usr/local/bin/flash-server"]
