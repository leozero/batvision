-- The version (file) a user plays for a movie or episode that has several, when it is
-- not the default one (the largest). Gone with the file: the default applies again.
CREATE TABLE version_choices (
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_kind       TEXT    NOT NULL CHECK (item_kind IN ('movie', 'episode')),
    item_id         INTEGER NOT NULL,
    media_file_id   INTEGER NOT NULL REFERENCES media_files(id) ON DELETE CASCADE,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (user_id, item_kind, item_id)
);
