-- Titles parsed by older versions could not be matched on TMDB; the improved
-- parser reads them differently, so search every unmatched item again.
UPDATE movies SET metadata_updated_at = NULL WHERE tmdb_id IS NULL AND identify_locked = 0;
UPDATE series SET metadata_updated_at = NULL WHERE tmdb_id IS NULL AND identify_locked = 0;
