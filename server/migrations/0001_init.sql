-- Accounts ---------------------------------------------------------------------

CREATE TABLE users (
    id              INTEGER PRIMARY KEY,
    username        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    password_hash   TEXT    NOT NULL,
    role            TEXT    NOT NULL CHECK (role IN ('admin', 'user')),
    -- TMDB language tag used for this user's metadata (fr-FR, en-US…).
    language        TEXT    NOT NULL DEFAULT 'fr-FR',
    -- Base32 TOTP secret. Set during enrollment, active only once totp_enabled = 1.
    totp_secret     TEXT,
    totp_enabled    INTEGER NOT NULL DEFAULT 0,
    -- Last accepted TOTP time step: a code is accepted only once (RFC 6238 §5.2).
    totp_last_step  INTEGER,
    created_at      INTEGER NOT NULL
);

CREATE TABLE recovery_codes (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash   TEXT    NOT NULL,
    used_at     INTEGER
);

-- One row per signed-in client installation. Only a hash of the token is stored.
CREATE TABLE devices (
    id              INTEGER PRIMARY KEY,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    platform        TEXT    NOT NULL,
    token_hash      TEXT    NOT NULL UNIQUE,
    created_at      INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    revoked_at      INTEGER
);

CREATE TABLE invitations (
    id          INTEGER PRIMARY KEY,
    token_hash  TEXT    NOT NULL UNIQUE,
    role        TEXT    NOT NULL CHECK (role IN ('admin', 'user')),
    created_by  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    used_at     INTEGER,
    used_by     INTEGER REFERENCES users(id) ON DELETE SET NULL
);

-- Library ----------------------------------------------------------------------

CREATE TABLE libraries (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    kind        TEXT    NOT NULL CHECK (kind IN ('movies', 'series')),
    -- Relative to the media root.
    path        TEXT    NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL,
    last_scan_at INTEGER
);

CREATE TABLE movies (
    id                  INTEGER PRIMARY KEY,
    library_id          INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    -- Folder (or file, for loose files) the movie was detected from, relative to the media root.
    source_path         TEXT    NOT NULL UNIQUE,
    parsed_title        TEXT    NOT NULL,
    parsed_year         INTEGER,
    tmdb_id             INTEGER,
    -- Set by a manual identification: the scanner never overrides it.
    identify_locked     INTEGER NOT NULL DEFAULT 0,
    release_date        TEXT,
    runtime_minutes     INTEGER,
    poster_path         TEXT,
    backdrop_path       TEXT,
    metadata_updated_at INTEGER,
    added_at            INTEGER NOT NULL,
    missing_since       INTEGER
);

CREATE TABLE series (
    id                  INTEGER PRIMARY KEY,
    library_id          INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    source_path         TEXT    NOT NULL UNIQUE,
    parsed_title        TEXT    NOT NULL,
    parsed_year         INTEGER,
    tmdb_id             INTEGER,
    identify_locked     INTEGER NOT NULL DEFAULT 0,
    first_air_date      TEXT,
    poster_path         TEXT,
    backdrop_path       TEXT,
    metadata_updated_at INTEGER,
    added_at            INTEGER NOT NULL,
    missing_since       INTEGER
);

CREATE TABLE seasons (
    id              INTEGER PRIMARY KEY,
    series_id       INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    season_number   INTEGER NOT NULL,
    poster_path     TEXT,
    UNIQUE (series_id, season_number)
);

CREATE TABLE episodes (
    id              INTEGER PRIMARY KEY,
    series_id       INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    season_id       INTEGER NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    season_number   INTEGER NOT NULL,
    episode_number  INTEGER NOT NULL,
    -- For multi-episode files (S01E01E02): last episode covered.
    episode_number_end INTEGER,
    air_date        TEXT,
    runtime_minutes INTEGER,
    still_path      TEXT,
    added_at        INTEGER NOT NULL,
    missing_since   INTEGER,
    UNIQUE (series_id, season_number, episode_number)
);

-- Title/overview per language, so each user sees metadata in their own language.
CREATE TABLE translations (
    item_kind   TEXT    NOT NULL CHECK (item_kind IN ('movie', 'series', 'season', 'episode')),
    item_id     INTEGER NOT NULL,
    language    TEXT    NOT NULL,
    title       TEXT,
    overview    TEXT,
    PRIMARY KEY (item_kind, item_id, language)
);

-- One playable file. A movie or an episode points to exactly one file.
CREATE TABLE media_files (
    id              INTEGER PRIMARY KEY,
    -- Relative to the media root.
    path            TEXT    NOT NULL UNIQUE,
    item_kind       TEXT    NOT NULL CHECK (item_kind IN ('movie', 'episode')),
    item_id         INTEGER NOT NULL,
    size_bytes      INTEGER NOT NULL,
    modified_at     INTEGER NOT NULL,
    duration_ms     INTEGER,
    -- Raw `ffprobe -show_streams -show_format` JSON, used for playback decisions.
    probe_json      TEXT,
    probe_error     TEXT,
    missing_since   INTEGER
);

CREATE INDEX media_files_item ON media_files (item_kind, item_id);
CREATE INDEX episodes_series ON episodes (series_id, season_number, episode_number);
