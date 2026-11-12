-- Series in the watchlist (D73): the next unwatched episodes download on devices with
-- automatic downloads, like watchlist movies. Account-wide, unlike series_subscriptions.
CREATE TABLE watchlist_series (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    series_id   INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (user_id, series_id)
);

-- How many unwatched episodes of each watchlist series to keep (D73).
ALTER TABLE users ADD COLUMN watchlist_episodes INTEGER NOT NULL DEFAULT 2
    CHECK (watchlist_episodes BETWEEN 1 AND 10);
