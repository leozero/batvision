-- Sagas (D101): TMDB movie collections (The Lord of the Rings, Back to the Future…).
CREATE TABLE collections (
    id                  INTEGER PRIMARY KEY,
    tmdb_id             INTEGER NOT NULL UNIQUE,
    poster_path         TEXT,
    backdrop_path       TEXT,
    -- Released movies TMDB lists in the collection, in the library or not. NULL until
    -- the collection itself has been fetched.
    parts_count         INTEGER,
    metadata_updated_at INTEGER
);

-- Name/overview per language, like `translations` for library items.
CREATE TABLE collection_translations (
    collection_id   INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    language        TEXT    NOT NULL,
    name            TEXT,
    overview        TEXT,
    PRIMARY KEY (collection_id, language)
);

ALTER TABLE movies ADD COLUMN collection_id INTEGER REFERENCES collections(id) ON DELETE SET NULL;
CREATE INDEX movies_collection ON movies (collection_id);
-- The public image route accepts collection images too.
CREATE INDEX collections_poster ON collections (poster_path);
CREATE INDEX collections_backdrop ON collections (backdrop_path);

-- Matched movies don't know their collection yet: mark them stale so scans refetch
-- them, a few at a time, oldest first.
UPDATE movies SET metadata_updated_at = 0 WHERE tmdb_id IS NOT NULL;
