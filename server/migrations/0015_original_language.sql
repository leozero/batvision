-- The language a movie or series was made in (TMDB's original_language, as ISO 639-2):
-- the "original version" audio preference plays it (D103). Matched items learn it at
-- their next metadata refresh, a few per scan.
ALTER TABLE movies ADD COLUMN original_language TEXT;
ALTER TABLE series ADD COLUMN original_language TEXT;
UPDATE movies SET metadata_updated_at = 0 WHERE tmdb_id IS NOT NULL;
UPDATE series SET metadata_updated_at = 0 WHERE tmdb_id IS NOT NULL;
