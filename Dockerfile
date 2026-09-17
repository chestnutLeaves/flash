# Flash, self-hosted: the open server on a slim Debian base.
#
#   docker build -t flash .
#   docker run -d -p 127.0.0.1:8437:8437 -v flash-data:/data \
#       -e FLASH_BASE_URL=https://cards.example.com flash
#
# The binary embeds its templates and assets; /data holds the SQLite
# database, media and import scratch space. The first boot logs the
# admin enrollment link.
#
# The image ships Pdfium, the PDF renderer behind the `pdf` generation
# input, as a shared library the server loads at boot. That rules out
# the static musl binary this image used to be built as: a static
# executable cannot dlopen, and a dynamically linked musl build would
# need Pdfium's musl variant plus a C++ runtime that Alpine packages
# separately. A glibc build on the same Debian the compiler image runs
# is the plain path, and bookworm-slim already carries the C++ runtime
# Pdfium needs.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p flash-server \
    && install -m 0755 target/release/flash-server /flash-server

# Pdfium from the open-source pdfium-binaries project, pinned to one
# release so the image is reproducible; bump PDFIUM_RELEASE to update.
# TARGETARCH is set by BuildKit (amd64, arm64).
FROM debian:bookworm-slim AS pdfium
ARG PDFIUM_RELEASE=chromium/8044
ARG TARGETARCH=amd64
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && case "$TARGETARCH" in amd64) arch=x64 ;; arm64) arch=arm64 ;; \
         *) echo "no pdfium build for $TARGETARCH" >&2; exit 1 ;; esac \
    && curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_RELEASE}/pdfium-linux-${arch}.tgz" \
       | tar -xz -C /tmp lib/libpdfium.so \
    && install -m 0644 /tmp/lib/libpdfium.so /libpdfium.so

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
COPY --from=pdfium /libpdfium.so /usr/local/lib/libpdfium.so
USER flash
# No office converter is baked in: point FLASH_DOC_CONVERTER at one of
# your own (a LibreOffice sidecar, say) to take Word and PowerPoint files.
ENV FLASH_DATA_DIR=/data \
    FLASH_BIND=0.0.0.0:8437 \
    FLASH_PDFIUM_PATH=/usr/local/lib/libpdfium.so
VOLUME /data
EXPOSE 8437
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
    CMD wget -qO- http://127.0.0.1:8437/healthz | grep -q '"db":"ok"' || exit 1
ENTRYPOINT ["/usr/local/bin/flash-server"]
