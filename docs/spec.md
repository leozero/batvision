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
apps/mobile/            Expo (Android + Android TV), Kotlin sync engine in modules/
packages/api-client/    generated from the OpenAPI description, never edited by hand
packages/bridge/        types shared by the desktop app and the web client
packages/core/          pure logic shared by the clients
packages/i18n/          interface translations
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
| **Watchlist**           | Movies and series to watch, per user, stored on the server (D73). Its own page on the web, its own tab on Android.                                |
| **Series subscription** | A download request made from a series page, for a given user and device. Two modes: `rolling(X)` or `season(n)`.                                  |
| **Expected set**        | The items a device should have on its disk, computed by the server.                                                                               |
| **Encoding profile**    | `max`, `mobile` or `saver`.                                                                                                                       |
| **Variant**             | An encoded file, identified by its profile × codec pair.                                                                                          |

## 4. Library

- Files are recognized using the **Jellyfin/Plex naming convention**:
  - movies: `Movies/Title (Year)/…`;
  - series: `Shows/Name (Year)/Season 01/…S01E01…`.
  - a folder whose files name two dated movies or more (a saga, a box set) gives one movie per file; parts (`CD1`, `CD2`) and copies of one movie stay one movie (D83);
  - videos in a folder inside a movie's folder are its bonuses, not movies (D83);
  - a file whose name isn't UTF-8 (Latin-1) is skipped with a warning in the logs: rename it.
- An **"Identify manually"** action in the UI fixes a wrong match.
- Initial scan, then detection of additions, changes and removals.
- **Metadata and images come from TMDB**, in each user's language (French by default), with English as a fallback. Images are cached in `data`.
- Matched items are **fetched from TMDB again every 30 days**, a few per scan, oldest first.
- **Ratings** (D59): TMDB's average out of 10 for movies, series, seasons and episodes, hidden when fewer than 5 people voted (a season counts its episodes' votes). The API returns a list per source, so other sources can be added later. They show on posters, detail pages, the season picker and episode rows, and the movie and series pages can sort by rating.
- **Sorting** (D75): the movie and series pages sort by date added, title, year or rating. Each sort has a natural direction (A to Z for titles, newest or best first for the others) that a button reverses; items without a year or rating stay last either way. The choice is remembered per page.
- **Top-rated movies** (D76): a home page rail with the 20 best-rated movies having at least 50 TMDB votes (fewer lets obscure titles in), watched ones included, and a link to the movies page sorted by rating.

## 5. Playback

- Each device sends its **capabilities**. The server then picks, in order:
  1. direct play of the file;
  2. remuxing into another container, without re-encoding;
  3. transcoding the audio only;
  4. full transcoding, as HLS with NVENC acceleration, burning in subtitles when needed. With an NVIDIA GPU, decoding, scaling and HDR tone mapping also run on it (D57).
- **Subtitles:**
  - text subtitles (SRT/ASS, external or embedded) are converted and rendered by the client;
  - image subtitles (PGS/VobSub) are burned into the video when the user picks them.
- **Audio and subtitle tracks:**
  - each user has general preferences: audio language, subtitle language, and display mode (`always`, `when the audio is not in my language`, `never`);
  - **when the user changes a track on an episode, the choice applies to the whole series**;
  - from one episode to the next, the track is matched by **language and kind** (normal, commentary, forced subtitles, subtitles for the deaf and hard of hearing), never by its index;
  - if the episode has no matching track, the general preferences apply.
- **Picture quality** (D60), chosen in the player and kept per device:
  - **Max**: the best the device can play, as above;
  - **Medium** (1080p, 6 Mbit/s), **Low** (720p, 2 Mbit/s, stereo audio) and **SOS** (480p, 1 Mbit/s, stereo audio): a full transcode, only when the file is heavier than the level, otherwise it plays as at Max;
  - **Auto** (the default) starts where it settled last time on the device. It goes down after two stalls within a minute, a stall of 4 s, a stream that shows nothing for 10 s, or a measured throughput under 1.5 times the stream's bitrate. It goes back up, straight to the best level the throughput carries twice over, after 2 minutes without a stall and at most once every 5 minutes;
  - changing level restarts the session at the current position, like a seek: a device still uses a single encode.
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
| Watchlist movies    | Every watchlist movie is expected on devices with automatic downloads enabled (off by default). **A watched movie leaves after 24 h.**    |
| Watchlist series    | The next X unwatched episodes, as `rolling(X)`, on devices with automatic downloads. X is an account setting, 1 to 10, 2 by default.      |
| Series `rolling(X)` | The next X unwatched episodes. The set refills as episodes are watched and new ones air, until the series ends or the subscription stops. |
| Series `season(n)`  | Every episode of season n, once. It never moves on to the next season.                                                                    |
| Manual download     | A movie or episode added by hand for one device.                                                                                          |

Series subscriptions and manual downloads are explicit requests for one device: they apply even with automatic downloads off. The audio track is chosen when requesting (default: the user's preferred audio language).

A series in the watchlist (D73) is not a subscription: it belongs to the account, applies to every device with automatic downloads, and uses the account's X. Its audio language is the one remembered for the series (D20), else the preferred one; its quality is the device's. It stays in the watchlist once everything is watched, so new episodes download when they arrive. When a subscription and the watchlist want the same episode, it downloads once, under the subscription. Watchlist entries are taken oldest first, movies and series together.

### 7.3 Deletion and quota

- **A watched item is deleted after 24 h** (configurable), whatever its source.
- **Per-device quota.** When exceeded, watched items go first, then the oldest ones.
- If several users on the same device want the same file, it is stored **only once**.
- **Cancel and delete** (D74): on the desktop and Android apps, each item can be cancelled before it's done, or deleted after. The transfer stops, the files (partial or not) go, and the server removes every source asking for that item on this device: the manual download, the subscription (a whole series or season), or the watchlist entry (on every device). A confirmation says which before doing it.

### 7.4 Network

- By default, downloads happen on Wi-Fi only. An "Only while charging" option exists per device, off by default (D69).
- **Mobile data:** per-device setting (D68): off (default), light versions only (`mobile`, `saver`), or all qualities.
  - Once enabled, it enforces a **monthly cap** (for example 2 GB), counted on the device per calendar month; at the cap, downloads wait for Wi-Fi.

### 7.5 Encoding profiles

| Profile  | Video                                       | Audio         |
| -------- | ------------------------------------------- | ------------- |
| `max`    | Original stream copied (H.264, HEVC or AV1) | AAC, 5.1 kept |
| `mobile` | 720p, about 1.5 Mb/s                        | AAC stereo    |
| `saver`  | 480p, about 700 kb/s                        | AAC stereo    |

- Output is always MP4 with **one audio track** (the chosen language), so it plays in any player.
- **Codec for lighter profiles:** AV1 if the device decodes it in hardware, otherwise HEVC. HDR is tone-mapped, on the GPU when there is one (D57).
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

### 7.9 Desktop client

- Ships its own build of the web interface. On first launch: server address, then sign-in (password + 2FA). The device token is encrypted with the OS keychain.
- Download features appear only in the desktop app: a "On this computer" section in Downloads, a "Downloaded" mark on detail pages.
- Files go to `Videos/Batvision` by default (changeable), with a 100 GB quota by default.
- When the server is unreachable, an **offline library** shows only downloaded items, played by the same player (tracks, local WebVTT subtitles, resume). Offline progress is uploaded when the server is back.
- Only downloaded items' metadata and images are kept, in a JSON file.
- Closing the window keeps the app in the tray; "Quit" stops it. Sync on open, when the network returns and every 30 minutes. Launch at login is off by default.
- One download at a time, pause/resume, interrupted downloads resume (HTTP Range). No network restriction.
- One signed-in account per install.
- Builds: macOS (arm64, x64), Windows, Linux AppImage, unsigned until milestone 7, published as GitHub releases.
- Updates (D58): Windows and Linux download new releases in the background and install them on quit or from the banner; macOS shows a banner linking to the release.

### 7.10 Android client

- Native React Native screens (Expo, expo-router, NativeWind): home, movies, series, search, details, player, watchlist, downloads and device settings, sign-in. Administration and account security stay on the web (D61, D62).
  - Tabs: Home, Movies, Series, Watchlist, Downloads; search is the magnifier at the top of Home, Movies and Series.
  - Device settings are rows showing the current value; each opens a sheet listing the options with a line explaining each one (quality, space, mobile data and its cap, storage).
- First launch: server address, then sign-in (password + 2FA). The device token is kept in the Android keystore (expo-secure-store). One account per install (D67).
- **Download engine in Kotlin** (D63), a local Expo module running on WorkManager so it works with the app closed: upload offline progress, fetch the plan, request, download with resume (HTTP Range), confirm, delete. It honours the device's network rules (Wi-Fi, mobile data mode and cap, charging) and shows a progress notification with Pause while downloading.
- Files go to the app's own folder on the phone or the SD card (setting `storage`), moved when it changes; 20 GB quota by default (D70). Downloaded items' metadata and images are kept next to them for the offline library.
- Sync on open, when the network returns and every 30 minutes (WorkManager may delay it). Syncs the app asks for don't wait for Android to validate internet access, so a server on the home network works without it. While the server encodes for the phone, the engine checks again every 15 s (up to 8 minutes) so the download starts as soon as the file is ready.
- Player: react-native-video (Media3) with HLS, audio and subtitle tracks, side-loaded WebVTT for downloads, the quality menu (D60), resume, next episode, picture in picture, landscape, screen kept on (D71).
- Offline library when the server is unreachable, as on desktop (D72).
- The app reports its model as device name and whether it decodes AV1 in hardware (MediaCodec).
- Distribution: signed APK attached to the GitHub releases; the app shows a banner when a newer one exists (D65, D66).

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
