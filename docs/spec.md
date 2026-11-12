# batvision — Specification

> Self-hosted media server, rewritten from scratch with Jellyfin as inspiration, with web, desktop, Android and Android TV clients.
> What sets it apart: **automatic downloads** and **lighter versions encoded on demand**.
>
> Status: specification approved on 2026-11-11. Choices and their reasons are in [decisions.md](decisions.md).

## 1. Vision and scope

- **Audience:** personal and family use first, with several users. Then open-source release and Play Store publication.
- **Full replacement for Jellyfin**, limited to **movies and TV series**.
- **Code mostly written by AI.** Tests are therefore mandatory: unit tests, and end-to-end (e2e) tests for critical flows.
- **When to retire Jellyfin** is up to the server owner. It is not a project deliverable.

### Out of scope for v1

- Intro skipping.
- Trickplay (image previews in the seek bar).
- Collections.
- Parental controls.
- Synchronized group playback.
- Music, live TV, books.
- Chromecast.
- Reading NFO files.
- Plugins.
- Import or export from Jellyfin (planned later).
- Push notifications.
- Passkeys.

## 2. Architecture

```
┌──────────── Dokploy (Traefik: domain + HTTPS) ────────────────┐
│  batvision-server (Rust)                                      │
│   ├─ REST API + OpenAPI description                           │
│   ├─ library scanner + TMDB                                   │
│   ├─ playback: direct / remuxed / transcoded (HLS)            │
│   ├─ download engine: expected set per device                 │
│   ├─ ffmpeg encoding queue (NVENC, NVIDIA GPU)                │
│   └─ SQLite                                                   │
│  volumes: media (read-only) · data · cache                    │
└───────────────────────────────────────────────────────────────┘
        ▲                ▲                  ▲              ▲
     web (TS)     desktop Electron    Android (Expo)   Android TV (Expo)
  no downloads    downloads + offline  downloads + offline
```

### Stack

| Part                       | Choice                                                                                                      |
| -------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Server                     | Rust, axum, SQLite, ffmpeg ≥ 7.1 run as an external process, OpenAPI description generated from the code    |
| API client                 | TypeScript, generated from the OpenAPI description (`packages/api-client`)                                  |
| Web                        | TypeScript/React, hls.js for playback                                                                       |
| Desktop                    | Electron, reusing the web code, HTML5 player                                                                |
| Android / Android TV       | Expo (React Native, react-native-tvos for TV), native ExoPlayer, WorkManager for background work            |
| TypeScript package manager | pnpm                                                                                                        |
| CI                         | GitHub Actions                                                                                              |
| Tests                      | Rust: unit and integration tests. TS: unit tests. e2e: Playwright (web and Electron), then Maestro (mobile) |
| Licenses                   | AGPL-3.0 for the server, GPL-3.0 for the clients                                                            |

### Repository layout

```
server/                 Rust project
apps/web/
apps/desktop/           Electron
apps/mobile/            Expo (Android + Android TV)
packages/api-client/    generated from the OpenAPI description, never edited by hand
packages/ui-logic/      shared UI logic
docs/
```

### Deployment

- CI builds the server Docker image, with an ffmpeg built for NVENC, NVDEC and `scale_cuda`, and publishes it to the GitHub registry (GHCR).
- Dokploy deploys a `docker-compose` file that reserves the GPU (`nvidia-container-toolkit` installed on the host).
- Traefik, managed by Dokploy, handles the domain and HTTPS. The Rust server never handles TLS itself.
- Volumes:
  - `media`: movie and series folders, read-only;
  - `data`: SQLite database and images, **to be backed up**;
  - `cache`: transcoded and encoded files, on a fast disk, not backed up.

## 3. Glossary

| Term                    | Definition                                                                                                                                        |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| **User**                | An account, with the `admin` or `user` role.                                                                                                      |
| **Device**              | A client installation signed in as a user. It has its own revocable token.                                                                        |
| **Capabilities**        | What a device says it can play: containers, video and audio codecs (with or without hardware decoding), maximum resolution, subtitles it renders. |
| **Watchlist**           | A list of **movies only**, per user, stored on the server.                                                                                        |
| **Series subscription** | A download request made from a series page, for a given user and device. Two modes: `rolling(X)` or `season(n)`.                                  |
| **Expected set**        | The items a device should have on its disk, computed by the server.                                                                               |
| **Encoding profile**    | `max`, `mobile` or `saver`.                                                                                                                       |
| **Variant**             | An encoded file, identified by its profile × codec pair.                                                                                          |

## 4. Library

- Files are recognized using the **Jellyfin/Plex naming convention**:
  - movies: `Movies/Title (Year)/…`;
  - series: `Shows/Name (Year)/Season 01/…S01E01…`.
- An **"Identify manually"** action in the UI fixes a wrong match.
- Initial scan, then detection of additions, changes and removals.
- **Metadata and images come from TMDB**, in each user's language (French by default), with English as a fallback. Images are cached in `data`.

## 5. Playback

- Each device sends its **capabilities**. The server then picks, in order:
  1. direct play of the file;
  2. remuxing into another container, without re-encoding;
  3. transcoding the audio only;
  4. full transcoding, as HLS with NVENC acceleration, burning in subtitles when needed.
- **Subtitles:**
  - text subtitles (SRT/ASS, external or embedded) are converted and rendered by the client;
  - image subtitles (PGS/VobSub) are burned into the video when the user picks them.
- **Audio and subtitle tracks:**
  - each user has general preferences: audio language, subtitle language, and display mode (`always`, `when the audio is not in my language`, `never`);
  - **when the user changes a track on an episode, the choice applies to the whole series**;
  - from one episode to the next, the track is matched by **language and kind** (normal, commentary, forced subtitles, subtitles for the deaf and hard of hearing), never by its index;
  - if the episode has no matching track, the general preferences apply.
- Resume playback, "Continue watching", "Next up".

## 6. Authentication and security

The server is exposed to the internet.

- **No public sign-up.** The admin creates accounts or sends a single-use invitation link.
- **Sign-in:** username and password, hashed with argon2id.
- **2FA:** a code from an authenticator app (TOTP), with recovery codes. Optional for users, **mandatory for admins**. Only asked when signing in a device.
- **One token per device**, long-lived and revocable. Users see their device list and can revoke any of them.
- **Quick connect (TV):**
  1. the device shows a 6-character code, valid for 5 minutes;
  2. the user approves it from an already signed-in device;
  3. the device receives its own token.
- **Attack protection:** attempt limits per IP and per account, with a delay that grows after each failure.
- **Playback and downloads:** media URLs are signed and short-lived, because browser `<video>` tags cannot send an authentication header. Downloads accept HTTP Range requests, so an interrupted download can resume.

## 7. Downloads

### 7.1 Principle

**The server computes the expected set** for each user × device pair. The client just applies that set:

- it downloads what is missing;
- it deletes what is no longer in the set;
- it respects its local constraints: network, quota, storage.

All the logic deciding what to download and what to delete lives in **one place**, on the server, as pure functions covered by exhaustive unit tests.

### 7.2 Sources of the expected set

| Source              | Rule                                                                                                                                      |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Watchlist (movies)  | Every watchlist movie is expected on devices with automatic downloads enabled (off by default). **A watched movie leaves after 24 h.**    |
| Series `rolling(X)` | The next X unwatched episodes. The set refills as episodes are watched and new ones air, until the series ends or the subscription stops. |
| Series `season(n)`  | Every episode of season n, once. It never moves on to the next season.                                                                    |
| Manual download     | A movie or episode added by hand for one device.                                                                                          |

Series subscriptions and manual downloads are explicit requests for one device: they apply even with automatic downloads off. The audio track is chosen when requesting (default: the user's preferred audio language).

### 7.3 Deletion and quota

- **A watched item is deleted after 24 h** (configurable), whatever its source.
- **Per-device quota.** When exceeded, watched items go first, then the oldest ones.
- If several users on the same device want the same file, it is stored **only once**.

### 7.4 Network

- By default, downloads happen on Wi-Fi only (and while charging when possible).
- **Mobile data:** per-device setting, off by default.
  - Once enabled, it enforces a **monthly cap** (for example 2 GB).
  - On mobile data, only **lighter versions** are downloaded.

### 7.5 Encoding profiles

| Profile  | Video                                       | Audio         |
| -------- | ------------------------------------------- | ------------- |
| `max`    | Original stream copied (H.264, HEVC or AV1) | AAC, 5.1 kept |
| `mobile` | 720p, about 1.5 Mb/s                        | AAC stereo    |
| `saver`  | 480p, about 700 kb/s                        | AAC stereo    |

- Output is always MP4 with **one audio track** (the chosen language), so it plays in any player.
- **Codec for lighter profiles:** AV1 if the device decodes it in hardware, otherwise HEVC. HDR is tone-mapped.
- Default profile per device (desktop: `max`, Android: `mobile`), overridable per series or manual download.
- **All text subtitles** (embedded and external) are delivered as separate WebVTT files.

### 7.6 Server-side encoding

- **Encoding only when a device asks** (no anticipation: too heavy for a home server), in a persistent queue.
  - One encode at a time by default (`BATVISION_ENCODE_CONCURRENCY`), leaving room for live playback.
- **Variant cache:**
  - size budget, 50 GB by default (`BATVISION_ENCODE_CACHE_GB`);
  - **a variant is deleted as soon as every device that asked for it confirmed the download**;
  - over budget, only variants nobody is waiting for are evicted.
- Finished variants are complete files, with a known size, downloadable with resume (HTTP Range).
- Device protocol: `POST /api/downloads/plan` (expected set with each item's state) → `POST /api/downloads/request` → download `url` and `subtitles` when `ready` → `POST /api/downloads/confirm`.

### 7.7 Synchronization

- **No push notifications.** The client checks on its own:
  - when the app opens;
  - when the connection comes back;
  - periodically, every 15 minutes to 1 hour (WorkManager on Android, a timer on desktop).
- At each check, the client:
  1. **uploads progress made offline** (positions, watched state, watch dates) with `POST /api/progress/batch`;
  2. fetches its expected set;
  3. applies it.
- **On a progress conflict**, the most recent watch date wins.

### 7.8 Role of each client

| Client             | Downloads       | Role                                                                                                 |
| ------------------ | --------------- | ---------------------------------------------------------------------------------------------------- |
| Web                | No              | Playback, plus control: watchlist, series subscriptions, device rules, quotas, encoding queue, admin |
| Desktop (Electron) | Yes             | First offline client. It validates the download engine                                               |
| Android (Expo)     | Yes             | Storage in the app's private space **or** on the SD card, chosen per device                          |
| Android TV (Expo)  | To decide later | Playback, quick connect                                                                              |

## 8. Milestones

| #   | Milestone        | Content                                                                                                           |
| --- | ---------------- | ----------------------------------------------------------------------------------------------------------------- |
| 0   | Foundations      | Repository, CI, Docker image with NVENC, first end-to-end Dokploy deployment                                      |
| 1   | Server core      | Accounts, 2FA, device tokens, quick connect (server side), folder scanning, TMDB, OpenAPI and generated TS client |
| 2   | Web              | Library browsing, playback (direct or transcoded), subtitles and tracks, progress, admin                          |
| 3   | Download engine  | Watchlist, series subscriptions, device rules, expected set, encoding queue, variant cache, control from the web  |
| 4   | Electron desktop | Downloads, offline playback, sync, automatic deletion                                                             |
| 5   | Android          | Background downloads, mobile data, quota, SD card                                                                 |
| 6   | Android TV       | TV interface, quick connect screen                                                                                |
| 7   | Release          | Open-source release, Play Store                                                                                   |

## 9. To verify during implementation

- The actual number of concurrent NVENC sessions allowed by the host's driver.
- Support for the Blackwell generation (RTX 50xx) in the chosen ffmpeg version.
- WorkManager behavior and background work limits on the targeted Android versions.
