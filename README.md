# batvision

Self-hosted media server (movies and TV series), rewritten from scratch with Jellyfin as inspiration. What sets it apart: **automatic downloads** and **lighter versions encoded on demand**.

- Specification: [docs/spec.md](docs/spec.md)
- Decision log: [docs/decisions.md](docs/decisions.md)

> Early development: only the project foundations exist so far.

## Layout

| Folder                 | Content                                                  | License  |
| ---------------------- | -------------------------------------------------------- | -------- |
| `server/`              | Rust server (axum, SQLite, ffmpeg)                       | AGPL-3.0 |
| `apps/web/`            | Web client (React, Vite)                                 | GPL-3.0  |
| `packages/api-client/` | TypeScript client generated from the OpenAPI description | GPL-3.0  |

## Development

Requirements: stable Rust, Node 24, pnpm 12 (`npm i -g pnpm@12`), ffmpeg on the `PATH`.

```bash
pnpm install
```

```bash
cargo run --manifest-path server/Cargo.toml
```

```bash
pnpm --filter @batvision/web dev
```

The server listens on `127.0.0.1:8420`, and Vite (`http://localhost:5173`) proxies `/api` calls to it.

### Tests

```bash
pnpm lint && pnpm typecheck && pnpm test
```

```bash
cargo test --manifest-path server/Cargo.toml
```

```bash
pnpm --filter @batvision/web build && pnpm test:e2e
```

### After changing the server API

```bash
pnpm --filter @batvision/api-client generate
```

## Server configuration

| Variable              | Default (dev)  | Purpose                                                |
| --------------------- | -------------- | ------------------------------------------------------ |
| `BATVISION_BIND`      | `0.0.0.0:8420` | Listen address (HTTPS is handled by the reverse proxy) |
| `BATVISION_WEB_DIR`   | —              | Built web client, served as a SPA                      |
| `BATVISION_DATA_DIR`  | `./.dev/data`  | Database and images, **to be backed up**               |
| `BATVISION_MEDIA_DIR` | `./.dev/media` | Media library, read-only                               |
| `BATVISION_CACHE_DIR` | `./.dev/cache` | Transcodes and encoded versions, disposable            |
| `BATVISION_FFMPEG`    | `ffmpeg`       | ffmpeg binary                                          |
| `RUST_LOG`            | `info`         | Log level                                              |

## Deployment (Dokploy)

1. **Image:** every push to `main` publishes `ghcr.io/<owner>/batvision:latest` (`Docker image` workflow). If the GHCR package is private, add a GHCR registry in Dokploy with a token that has `read:packages`.
2. **Host:** `nvidia-container-toolkit` must be installed.
3. **In Dokploy:** create a **Compose** service pointing at this repository, using `docker-compose.yml`.
4. **Variables:** copy [.env.example](.env.example) into the _Environment_ tab and adjust the paths.
5. **Domain:** add your domain on the `batvision` service, port `8420`, with HTTPS. Dokploy configures Traefik.
6. **Check:** `https://<domain>/` should show the server version and, under _Encoders_, `av1_nvenc`, `hevc_nvenc` and `h264_nvenc`. If they're missing, the GPU is not reachable from the container.

> `/api/system/info` is public during milestone 0. It becomes admin-only in milestone 1.
