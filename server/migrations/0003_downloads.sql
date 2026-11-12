-- Download settings of a client installation (desktop, Android…).
CREATE TABLE device_settings (
    device_id           INTEGER PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    -- Watchlist movies are downloaded only where this is on. Off by default.
    auto_download       INTEGER NOT NULL DEFAULT 0,
    profile             TEXT    NOT NULL CHECK (profile IN ('max', 'mobile', 'saver')),
    -- Space the device may use for downloads; NULL = no limit.
    quota_bytes         INTEGER,
    mobile_data         INTEGER NOT NULL DEFAULT 0,
    mobile_cap_bytes    INTEGER,
    storage             TEXT    NOT NULL DEFAULT 'internal' CHECK (storage IN ('internal', 'sd')),
    -- Hardware AV1 decoding, as last reported by the device: picks AV1 over HEVC.
    av1                 INTEGER NOT NULL DEFAULT 0,
    updated_at          INTEGER NOT NULL
);

-- Movies to download automatically. A watched movie stays 24 h, then leaves.
CREATE TABLE watchlist (
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    movie_id        INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
    -- ISO 639-2 audio language chosen when adding; NULL = the file's default track.
    audio_language  TEXT,
    added_at        INTEGER NOT NULL,
    PRIMARY KEY (user_id, movie_id)
);

-- A series downloaded on one device: the next X unwatched episodes, or one season.
CREATE TABLE series_subscriptions (
    id              INTEGER PRIMARY KEY,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id       INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    series_id       INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    mode            TEXT    NOT NULL CHECK (mode IN ('rolling', 'season')),
    -- rolling: how many unwatched episodes to keep.
    count           INTEGER,
    -- season: which season.
    season_number   INTEGER,
    -- NULL = the device's profile.
    profile         TEXT CHECK (profile IN ('max', 'mobile', 'saver')),
    audio_language  TEXT,
    created_at      INTEGER NOT NULL,
    UNIQUE (device_id, series_id, mode, season_number)
);

-- A movie or episode requested by hand for one device.
CREATE TABLE manual_downloads (
    id              INTEGER PRIMARY KEY,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id       INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    item_kind       TEXT    NOT NULL CHECK (item_kind IN ('movie', 'episode')),
    item_id         INTEGER NOT NULL,
    profile         TEXT CHECK (profile IN ('max', 'mobile', 'saver')),
    audio_language  TEXT,
    created_at      INTEGER NOT NULL,
    UNIQUE (device_id, item_kind, item_id)
);

-- An encoded file: one media file × profile × codec × audio track.
CREATE TABLE variants (
    id              INTEGER PRIMARY KEY,
    media_file_id   INTEGER NOT NULL REFERENCES media_files(id) ON DELETE CASCADE,
    profile         TEXT    NOT NULL CHECK (profile IN ('max', 'mobile', 'saver')),
    codec           TEXT    NOT NULL,
    audio_index     INTEGER,
    status          TEXT    NOT NULL CHECK (status IN ('queued', 'encoding', 'ready', 'failed')),
    progress        REAL    NOT NULL DEFAULT 0,
    size_bytes      INTEGER,
    -- JSON list of {file, language, kind} for the WebVTT subtitles next to the video.
    subtitles_json  TEXT,
    error           TEXT,
    created_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    UNIQUE (media_file_id, profile, codec, audio_index)
);

-- Which device asked for which variant, and whether it confirmed the download.
-- A variant is deleted from the cache once every requesting device confirmed.
CREATE TABLE variant_requests (
    variant_id      INTEGER NOT NULL REFERENCES variants(id) ON DELETE CASCADE,
    device_id       INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    requested_at    INTEGER NOT NULL,
    confirmed_at    INTEGER,
    PRIMARY KEY (variant_id, device_id)
);

-- What each device has downloaded. Survives the deletion of the cache copy; rows for
-- items no longer expected are pruned when the device's plan is computed.
CREATE TABLE device_downloads (
    device_id       INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    item_kind       TEXT    NOT NULL CHECK (item_kind IN ('movie', 'episode')),
    item_id         INTEGER NOT NULL,
    profile         TEXT    NOT NULL,
    size_bytes      INTEGER,
    downloaded_at   INTEGER NOT NULL,
    PRIMARY KEY (device_id, item_kind, item_id)
);
