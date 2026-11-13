# BatVision

Self-hosted media server (movies and TV series), rewritten from scratch with Jellyfin as inspiration. What sets it apart: **automatic downloads** and **lighter versions encoded on demand**.

- Specification: [docs/spec.md](docs/spec.md)
- Decision log: [docs/decisions.md](docs/decisions.md)

> Early development: the server, the web client, the download engine, the desktop app and the Android app work; the iOS and iPadOS app is being built.

## Layout

| Folder                 | Content                                                            | License  |
| ---------------------- | ------------------------------------------------------------------ | -------- |
| `server/`              | Rust server (axum, SQLite, ffmpeg)                                 | AGPL-3.0 |
| `apps/web/`            | Web client (React, Vite)                                           | GPL-3.0  |
| `apps/desktop/`        | Desktop app (Electron): the web client plus downloads              | GPL-3.0  |
| `apps/mobile/`         | Android and iOS app (Expo, React Native), Kotlin and Swift engines | GPL-3.0  |
| `packages/api-client/` | TypeScript client generated from the OpenAPI description           | GPL-3.0  |
| `packages/bridge/`     | Types shared by the desktop app and the web client                 | GPL-3.0  |
| `packages/core/`       | Pure logic shared by the clients (formats, quality)                | GPL-3.0  |
| `packages/i18n/`       | Interface translations (French, English)                           | GPL-3.0  |

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

### Desktop app

The desktop app shows the web client's own build, so build it first:

```bash
pnpm --filter @batvision/web build && pnpm --filter @batvision/desktop build
```

```bash
pnpm --filter @batvision/desktop start
```

On first launch it asks for the server address (`http://127.0.0.1:8420` for a local server). Downloads go to `Videos/BatVision` by default. Installers for macOS, Windows and Linux (unsigned for now) are on the [releases page](https://github.com/leozero/batvision/releases), or built locally with `pnpm --filter @batvision/desktop dist`. To publish a release, set the version in `apps/desktop/package.json`, `apps/mobile/package.json`, `apps/mobile/app.json` and `server/Cargo.toml`, and push a matching `v*` tag: installed apps pick it up (Windows and Linux update themselves, macOS shows a download banner).

### Android app

Needs the Android SDK and JDK 17 or later (Android Studio's own is fine: `export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"`). The `android/` folder is generated, never edited by hand:

```bash
pnpm --filter @batvision/mobile build:apk
```

```bash
adb install -r apps/mobile/android/app/build/outputs/apk/release/app-release.apk
```

During development, `pnpm --filter @batvision/mobile android` runs a debug build with live reload on a device or an emulator. On first launch the app asks for the server address (`http://10.0.2.2:8420` reaches a local server from the emulator). Signed APKs are attached to the releases; they need the release key in the repository secrets (`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`), and `apps/mobile/app.json` and `package.json` must carry the release version.

### iOS and iPadOS app

Needs Xcode and CocoaPods (`brew install cocoapods`). The `ios/` folder is generated, never edited by hand. A build for the simulator:

```bash
cd apps/mobile && pnpm exec expo prebuild --platform ios
```

```bash
cd apps/mobile/ios && xcodebuild -workspace BatVision.xcworkspace -scheme BatVision -configuration Release -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' -derivedDataPath build/dd build
```

During development, `pnpm --filter @batvision/mobile ios` runs a debug build with live reload on a simulator. The simulator reaches a local server at `http://127.0.0.1:8420`; plain HTTP is allowed on the local network only (D97). Release tags upload a build to TestFlight (D94) once the repository has an App Store Connect API key with the Admin role (`APP_STORE_CONNECT_KEY`, the `.p8` file's content; `APP_STORE_CONNECT_KEY_ID`; `APP_STORE_CONNECT_ISSUER_ID`) and the team id (`APPLE_TEAM_ID`).

### Tests

```bash
pnpm lint && pnpm typecheck && pnpm test
```

```bash
cargo test --manifest-path server/Cargo.toml
```

```bash
pnpm --filter @batvision/web build && pnpm --filter @batvision/web test:e2e
```

The desktop scenario (needs ffmpeg; on Linux, run it under `xvfb-run`):

```bash
pnpm --filter @batvision/desktop build && pnpm --filter @batvision/desktop test:e2e
```

The Android scenario (needs ffmpeg, a running emulator and [Maestro](https://github.com/mobile-dev-inc/maestro)), after building the APK:

```bash
cd apps/mobile/android && ./gradlew :batvision-sync:testDebugUnitTest
```

```bash
pnpm --filter @batvision/mobile test:e2e
```

The iOS scenario, the same flows on a booted simulator, after the simulator build above; and the Swift engine's unit tests:

```bash
pnpm --filter @batvision/mobile test:e2e:ios
```

```bash
pnpm --filter @batvision/mobile test:engine:ios
```

### After changing the server API

```bash
pnpm --filter @batvision/api-client generate
```

## Server configuration

| Variable                          | Default (dev)  | Purpose                                                                         |
| --------------------------------- | -------------- | ------------------------------------------------------------------------------- |
| `BATVISION_BIND`                  | `0.0.0.0:8420` | Listen address (HTTPS is handled by the reverse proxy)                          |
| `BATVISION_WEB_DIR`               | —              | Built web client, served as a SPA                                               |
| `BATVISION_DATA_DIR`              | `./.dev/data`  | Database and images, **to be backed up**                                        |
| `BATVISION_MEDIA_DIR`             | `./.dev/media` | Media library, read-only                                                        |
| `BATVISION_CACHE_DIR`             | `./.dev/cache` | Transcodes and encoded versions, disposable                                     |
| `BATVISION_FFMPEG`                | `ffmpeg`       | ffmpeg binary                                                                   |
| `BATVISION_FFPROBE`               | `ffprobe`      | ffprobe binary                                                                  |
| `BATVISION_TMDB_TOKEN`            | —              | TMDB "API Read Access Token". Without it, no metadata                           |
| `BATVISION_DEFAULT_LANGUAGE`      | `fr-FR`        | Default metadata language, also used for new accounts                           |
| `BATVISION_SCAN_INTERVAL_MINUTES` | `15`           | Interval between automatic scans (`0` disables them)                            |
| `BATVISION_TRUST_PROXY`           | `false`        | Read the client IP from `X-Forwarded-For` (only behind a reverse proxy)         |
| `BATVISION_SECURE_COOKIES`        | `true`         | `Secure` flag on the session cookie (disable only for plain-HTTP local testing) |
| `BATVISION_ENCODE_CACHE_GB`       | `50`           | Size budget of the download versions cache                                      |
| `BATVISION_ENCODE_CONCURRENCY`    | `1`            | Download versions encoded at the same time                                      |
| `BATVISION_SETUP_TOKEN`           | —              | Fixed first-run setup token (16+ characters) instead of a random one            |
| `RUST_LOG`                        | `info`         | Log level                                                                       |

## Deployment (Dokploy)

1. **Image:** every push to `main` publishes `ghcr.io/<owner>/batvision:latest` (`Docker image` workflow). If the GHCR package is private, add a GHCR registry in Dokploy with a token that has `read:packages`.
2. **Host:** `nvidia-container-toolkit` must be installed.
3. **In Dokploy:** create a **Compose** service pointing at this repository, using `docker-compose.yml`.
4. **Variables:** copy [.env.example](.env.example) into the _Environment_ tab and adjust the paths.
5. **Domain:** add your domain on the `batvision` service, port `8420`, with HTTPS. Dokploy configures Traefik.
6. **Check:** `https://<domain>/` should show the server version.

## First launch

1. On first start, the server logs: `no account exists yet: create the first administrator with setup token XXXXXXXX`.
2. Open the web app: it asks for that token and creates the administrator account. The token works once; if the server restarts before that, a new one is printed.
3. Enable two-factor authentication (Account → Security). It is mandatory for administrators: without it, administration is refused.
4. Add your libraries (Administration → Libraries) with a folder relative to `/media`, for example `Movies`. A first scan starts right away.
5. Administration → System lists the detected encoders: `av1_nvenc`, `hevc_nvenc` and `h264_nvenc` should be there. If they're missing, the GPU is not reachable from the container.

The full API documentation is served by the server at `/api/openapi.json`.
