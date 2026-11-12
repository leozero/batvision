# syntax=docker/dockerfile:1.7

# ---- Web client -------------------------------------------------------------
FROM node:24-slim AS web
WORKDIR /src
RUN npm install -g pnpm@12.18.0
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/
COPY packages/api-client/package.json packages/api-client/
COPY packages/bridge/package.json packages/bridge/
COPY packages/core/package.json packages/core/
COPY packages/i18n/package.json packages/i18n/
RUN --mount=type=cache,id=pnpm,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile --filter @batvision/web...
COPY packages/api-client packages/api-client
COPY packages/bridge packages/bridge
COPY packages/core packages/core
COPY packages/i18n packages/i18n
COPY apps/web apps/web
RUN pnpm --filter @batvision/web build

# ---- Server -----------------------------------------------------------------
FROM rust:1.99-slim-trixie AS server
WORKDIR /src/server
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/migrations migrations
COPY server/src src
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-target,target=/src/server/target \
    cargo build --release --locked \
    && cp target/release/batvision-server /batvision-server

# ---- Runtime ----------------------------------------------------------------
FROM debian:trixie-slim
ARG TARGETARCH
# jellyfin-ffmpeg: ffmpeg built for media servers (NVENC/NVDEC, tonemapping), pinned.
ARG JELLYFIN_FFMPEG_VERSION=8.1.3-2

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && curl -fsSL -o /tmp/ffmpeg.deb \
       "https://github.com/jellyfin/jellyfin-ffmpeg/releases/download/v${JELLYFIN_FFMPEG_VERSION}/jellyfin-ffmpeg8_${JELLYFIN_FFMPEG_VERSION}-trixie_${TARGETARCH}.deb" \
    && apt-get install -y --no-install-recommends /tmp/ffmpeg.deb \
    && rm -rf /tmp/ffmpeg.deb /var/lib/apt/lists/*

COPY --from=server /batvision-server /usr/local/bin/batvision-server
COPY --from=web /src/apps/web/dist /app/web

# NVIDIA container toolkit: expose the GPU's video engines (NVENC/NVDEC) to the container.
ENV NVIDIA_VISIBLE_DEVICES=all \
    NVIDIA_DRIVER_CAPABILITIES=compute,video,utility \
    BATVISION_BIND=0.0.0.0:8420 \
    BATVISION_WEB_DIR=/app/web \
    BATVISION_DATA_DIR=/data \
    BATVISION_CACHE_DIR=/cache \
    BATVISION_MEDIA_DIR=/media \
    BATVISION_FFMPEG=/usr/lib/jellyfin-ffmpeg/ffmpeg \
    BATVISION_FFPROBE=/usr/lib/jellyfin-ffmpeg/ffprobe
# CUDA compiles the GPU filters for the card on first use: keep them, or every
# ffmpeg start pays seconds of compilation.
ENV CUDA_CACHE_PATH=/cache/cuda

RUN mkdir -p /data /cache /media && chown 1000:1000 /data /cache
USER 1000:1000
VOLUME ["/data", "/cache"]
EXPOSE 8420
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
    CMD curl -fsS http://127.0.0.1:8420/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/batvision-server"]
