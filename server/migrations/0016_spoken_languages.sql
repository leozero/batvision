-- Track preferences become the languages someone understands, in order, and whether they
-- prefer a dubbed version of what they don't understand (D104, replacing D103's presets).
-- Existing choices carry over: their subtitle (else audio) language is the one they
-- understand, and picking an audio language meant preferring the dub.
ALTER TABLE users ADD COLUMN spoken_languages TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN prefer_dub INTEGER NOT NULL DEFAULT 0;
UPDATE users SET
    spoken_languages = COALESCE(subtitle_language, NULLIF(audio_language, 'original'), ''),
    prefer_dub = audio_language IS NOT NULL AND audio_language <> 'original';
ALTER TABLE users DROP COLUMN audio_language;
ALTER TABLE users DROP COLUMN subtitle_language;

-- A track choice made while watching a movie is kept for that movie, as one made during an
-- episode is for its series (D104).
CREATE TABLE movie_track_preferences (
    user_id           INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    movie_id          INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
    audio_language    TEXT,
    audio_kind        TEXT,
    -- NULL language with subtitles_off = 1 means "no subtitles".
    subtitle_language TEXT,
    subtitle_kind     TEXT,
    subtitles_off     INTEGER NOT NULL DEFAULT 0,
    updated_at        INTEGER NOT NULL,
    PRIMARY KEY (user_id, movie_id)
);
