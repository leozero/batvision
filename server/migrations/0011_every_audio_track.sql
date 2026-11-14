-- Downloads now carry every audio track (D40 revised): new variants have no audio index.
-- One-track versions are dropped (their folders go at startup), so a device that hasn't
-- finished downloading one asks again and gets every track. Copies already on devices stay.
DELETE FROM variants WHERE audio_index IS NOT NULL;
