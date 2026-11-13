-- The public image route checks that a path belongs to the library on every request.
CREATE INDEX movies_poster ON movies (poster_path);
CREATE INDEX movies_backdrop ON movies (backdrop_path);
CREATE INDEX series_poster ON series (poster_path);
CREATE INDEX series_backdrop ON series (backdrop_path);
CREATE INDEX seasons_poster ON seasons (poster_path);
CREATE INDEX episodes_still ON episodes (still_path);
