-- Sagas show the movies the library doesn't have, greyed out (D101): the released movies
-- TMDB lists in each collection, per language (titles and posters are translated).
CREATE TABLE collection_parts (
    collection_id   INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    tmdb_id         INTEGER NOT NULL,
    language        TEXT    NOT NULL,
    title           TEXT,
    release_date    TEXT,
    poster_path     TEXT,
    PRIMARY KEY (collection_id, tmdb_id, language)
);
-- The public image route accepts their posters too.
CREATE INDEX collection_parts_poster ON collection_parts (poster_path);

-- Known sagas don't have their movies yet: scans refetch them, a few at a time.
UPDATE collections SET metadata_updated_at = NULL;
