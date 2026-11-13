-- TMDB sends "" for titles without a date: stored as is, it read as year 0 and hid the
-- year parsed from the file name.
UPDATE movies SET release_date = NULL WHERE release_date = '';
UPDATE series SET first_air_date = NULL WHERE first_air_date = '';
