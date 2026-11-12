-- Audience/critic ratings, one row per item and source (only TMDB for now).
-- Stored whatever the vote count: the API hides ratings with too few votes.
CREATE TABLE ratings (
    item_kind   TEXT    NOT NULL CHECK (item_kind IN ('movie', 'series', 'season', 'episode')),
    item_id     INTEGER NOT NULL,
    -- 'tmdb'. No CHECK: adding a source must not need a table rebuild.
    source      TEXT    NOT NULL,
    value       REAL    NOT NULL,
    -- Best possible value: 10 for TMDB.
    scale       INTEGER NOT NULL,
    votes       INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (item_kind, item_id, source)
);

-- Matched items have no rating yet: mark them stale so scans refetch them,
-- a few at a time, oldest first.
UPDATE movies SET metadata_updated_at = 0 WHERE tmdb_id IS NOT NULL;
UPDATE series SET metadata_updated_at = 0 WHERE tmdb_id IS NOT NULL;
