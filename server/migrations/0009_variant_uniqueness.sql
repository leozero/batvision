-- UNIQUE treats NULL audio tracks as different: two devices asking for a silent video
-- got two encodes. Keep the oldest of each, then make the rule hold for NULL too.
DELETE FROM variants WHERE id NOT IN (
    SELECT MIN(id) FROM variants
    GROUP BY media_file_id, profile, codec, COALESCE(audio_index, -1)
);
CREATE UNIQUE INDEX variants_key ON variants
    (media_file_id, profile, codec, COALESCE(audio_index, -1));
