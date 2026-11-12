-- Per-user playback state of a movie or episode.
CREATE TABLE user_item_data (
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_kind       TEXT    NOT NULL CHECK (item_kind IN ('movie', 'episode')),
    item_id         INTEGER NOT NULL,
    -- Resume position. Reset to 0 once the item is played.
    position_ms     INTEGER NOT NULL DEFAULT 0,
    played          INTEGER NOT NULL DEFAULT 0,
    play_count      INTEGER NOT NULL DEFAULT 0,
    -- Last time it was watched (or marked played), Unix seconds. Drives "Continue watching".
    last_played_at  INTEGER,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (user_id, item_kind, item_id)
);
CREATE INDEX user_item_data_recent ON user_item_data (user_id, last_played_at);

-- General track preferences. Languages are ISO 639-2 codes as tagged in files (fre, eng…).
ALTER TABLE users ADD COLUMN audio_language TEXT;
ALTER TABLE users ADD COLUMN subtitle_language TEXT;
ALTER TABLE users ADD COLUMN subtitle_mode TEXT NOT NULL DEFAULT 'foreign'
    CHECK (subtitle_mode IN ('always', 'foreign', 'never'));

-- Track choice made while watching a series, reapplied to its other episodes.
CREATE TABLE series_track_preferences (
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    series_id       INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    audio_language  TEXT,
    audio_kind      TEXT,
    -- NULL language with subtitles_off = 1 means "no subtitles".
    subtitle_language TEXT,
    subtitle_kind   TEXT,
    subtitles_off   INTEGER NOT NULL DEFAULT 0,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (user_id, series_id)
);

-- Subtitle files next to a video (`Movie.fr.forced.srt`).
CREATE TABLE subtitle_files (
    id              INTEGER PRIMARY KEY,
    media_file_id   INTEGER NOT NULL REFERENCES media_files(id) ON DELETE CASCADE,
    path            TEXT    NOT NULL UNIQUE,
    format          TEXT    NOT NULL,
    language        TEXT,
    forced          INTEGER NOT NULL DEFAULT 0,
    sdh             INTEGER NOT NULL DEFAULT 0
);
