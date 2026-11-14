-- Timeline thumbnails (D99), in cache/trickplay/<media_file_id>/. Made for the file as it
-- was (size and modification time): a replaced file gets new ones.
CREATE TABLE trickplay (
    media_file_id   INTEGER PRIMARY KEY REFERENCES media_files(id) ON DELETE CASCADE,
    size_bytes      INTEGER NOT NULL,
    modified_at     INTEGER NOT NULL,
    -- Thumbnails made; NULL when the file has none (no video, ffmpeg failed).
    count           INTEGER,
    created_at      INTEGER NOT NULL
);
