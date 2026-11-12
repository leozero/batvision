//! The expected set: what one device should have on disk, in priority order.
//!
//! Pure function of plain data, so every rule is unit-tested. The client keeps the
//! longest prefix that fits its quota and deletes everything else: items it must
//! drop first (already watched) come last.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::userdata::PlayableKind;

/// A watched item stays on the device this long (to rewatch the end, or by mistake).
pub const KEEP_AFTER_WATCHED_SECS: i64 = 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Profile {
    /// Original video, audio converted to AAC, in MP4: full quality, plays everywhere.
    Max,
    /// 720p, about 1.5 Mb/s.
    Mobile,
    /// 480p, about 700 kb/s.
    Saver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Next unwatched episodes of a series.
    Rolling,
    /// A whole season.
    Season,
    /// Requested by hand.
    Manual,
    /// A watchlist movie (devices with automatic downloads only).
    Watchlist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Rolling { count: usize },
    Season { number: i64 },
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub series_id: i64,
    pub mode: Mode,
    pub profile: Option<Profile>,
    pub audio_language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WatchlistEntry {
    pub movie_id: i64,
    pub audio_language: Option<String>,
    pub added_at: i64,
}

#[derive(Debug, Clone)]
pub struct ManualEntry {
    pub kind: PlayableKind,
    pub id: i64,
    pub profile: Option<Profile>,
    pub audio_language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpisodeRef {
    pub id: i64,
    pub season: i64,
}

/// Watch state of one item for the device's user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemState {
    pub played: bool,
    pub position_ms: i64,
    pub last_played_at: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Inputs {
    pub now: i64,
    pub auto_download: bool,
    pub device_profile: Option<Profile>,
    pub subscriptions: Vec<Subscription>,
    pub watchlist: Vec<WatchlistEntry>,
    pub manual: Vec<ManualEntry>,
    /// Episodes of each subscribed series, in viewing order (specials last).
    pub series_episodes: HashMap<i64, Vec<EpisodeRef>>,
    pub states: HashMap<(PlayableKind, i64), ItemState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ExpectedItem {
    pub kind: PlayableKind,
    pub id: i64,
    pub profile: Profile,
    pub audio_language: Option<String>,
    pub source: Source,
    /// Watched: kept until `keep_until`, and the first to go when space runs out.
    pub watched: bool,
    pub keep_until: Option<i64>,
}

impl Inputs {
    fn state(&self, kind: PlayableKind, id: i64) -> ItemState {
        self.states.get(&(kind, id)).copied().unwrap_or_default()
    }

    /// `Some(keep_until)` for a watched item still in its 24 h window,
    /// `None` for an unwatched one; `Err(())` for a watched one past the window.
    fn watched_window(&self, kind: PlayableKind, id: i64) -> Result<Option<i64>, ()> {
        let state = self.state(kind, id);
        if !state.played {
            return Ok(None);
        }
        let until = state.last_played_at.unwrap_or(0) + KEEP_AFTER_WATCHED_SECS;
        if until > self.now {
            Ok(Some(until))
        } else {
            Err(())
        }
    }
}

pub fn expected_set(inputs: &Inputs) -> Vec<ExpectedItem> {
    let default_profile = inputs.device_profile.unwrap_or(Profile::Max);
    let mut wanted: Vec<ExpectedItem> = Vec::new();
    let mut watched: Vec<(i64, ExpectedItem)> = Vec::new();
    let mut seen: HashSet<(PlayableKind, i64)> = HashSet::new();

    let mut push = |kind: PlayableKind,
                    id: i64,
                    profile: Option<Profile>,
                    audio: &Option<String>,
                    source: Source,
                    window: Option<i64>,
                    wanted: &mut Vec<ExpectedItem>,
                    watched: &mut Vec<(i64, ExpectedItem)>| {
        if !seen.insert((kind, id)) {
            return;
        }
        let item = ExpectedItem {
            kind,
            id,
            profile: profile.unwrap_or(default_profile),
            audio_language: audio.clone(),
            source,
            watched: window.is_some(),
            keep_until: window,
        };
        match window {
            Some(until) => watched.push((until, item)),
            None => wanted.push(item),
        }
    };

    let ep = PlayableKind::Episode;

    // 1. Rolling series: the next unwatched episodes after the last one watched.
    for sub in &inputs.subscriptions {
        let Mode::Rolling { count } = sub.mode else {
            continue;
        };
        let episodes = inputs
            .series_episodes
            .get(&sub.series_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let anchor = episodes
            .iter()
            .enumerate()
            .filter_map(|(i, e)| inputs.state(ep, e.id).last_played_at.map(|at| (at, i)))
            .max()
            .map(|(_, i)| i);
        let start = match anchor {
            // Watched to the end: continue after it. In progress or stopped: it's next.
            Some(i) if inputs.state(ep, episodes[i].id).played => i + 1,
            Some(i) => i,
            None => 0,
        };
        let next = episodes[start.min(episodes.len())..]
            .iter()
            .filter(|e| !inputs.state(ep, e.id).played)
            .take(count);
        for e in next {
            push(
                ep,
                e.id,
                sub.profile,
                &sub.audio_language,
                Source::Rolling,
                None,
                &mut wanted,
                &mut watched,
            );
        }
        for e in episodes {
            if let Ok(Some(until)) = inputs.watched_window(ep, e.id) {
                push(
                    ep,
                    e.id,
                    sub.profile,
                    &sub.audio_language,
                    Source::Rolling,
                    Some(until),
                    &mut wanted,
                    &mut watched,
                );
            }
        }
    }

    // 2. Whole seasons (downloaded once: watched episodes leave after 24 h).
    for sub in &inputs.subscriptions {
        let Mode::Season { number } = sub.mode else {
            continue;
        };
        let episodes = inputs
            .series_episodes
            .get(&sub.series_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for e in episodes.iter().filter(|e| e.season == number) {
            if let Ok(window) = inputs.watched_window(ep, e.id) {
                push(
                    ep,
                    e.id,
                    sub.profile,
                    &sub.audio_language,
                    Source::Season,
                    window,
                    &mut wanted,
                    &mut watched,
                );
            }
        }
    }

    // 3. Manual downloads.
    for m in &inputs.manual {
        if let Ok(window) = inputs.watched_window(m.kind, m.id) {
            push(
                m.kind,
                m.id,
                m.profile,
                &m.audio_language,
                Source::Manual,
                window,
                &mut wanted,
                &mut watched,
            );
        }
    }

    // 4. Watchlist, on devices with automatic downloads, oldest additions first.
    if inputs.auto_download {
        let mut entries: Vec<&WatchlistEntry> = inputs.watchlist.iter().collect();
        entries.sort_by_key(|w| (w.added_at, w.movie_id));
        for w in entries {
            if let Ok(window) = inputs.watched_window(PlayableKind::Movie, w.movie_id) {
                push(
                    PlayableKind::Movie,
                    w.movie_id,
                    None,
                    &w.audio_language,
                    Source::Watchlist,
                    window,
                    &mut wanted,
                    &mut watched,
                );
            }
        }
    }

    // Watched items last; among them, the most recently watched are kept longest.
    watched.sort_by_key(|(until, item)| (std::cmp::Reverse(*until), item.id));
    wanted.extend(watched.into_iter().map(|(_, item)| item));
    wanted
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;
    const HOUR: i64 = 3600;

    fn episodes(spec: &[(i64, i64)]) -> Vec<EpisodeRef> {
        spec.iter()
            .map(|&(id, season)| EpisodeRef { id, season })
            .collect()
    }

    /// Series 1: S1E1..S1E4 (ids 11..14), S2E1..S2E2 (ids 21..22), special (id 1).
    fn base() -> Inputs {
        let mut inputs = Inputs {
            now: NOW,
            device_profile: Some(Profile::Mobile),
            ..Default::default()
        };
        inputs.series_episodes.insert(
            1,
            episodes(&[(11, 1), (12, 1), (13, 1), (14, 1), (21, 2), (22, 2), (1, 0)]),
        );
        inputs
    }

    fn rolling(count: usize) -> Subscription {
        Subscription {
            series_id: 1,
            mode: Mode::Rolling { count },
            profile: None,
            audio_language: Some("eng".into()),
        }
    }

    fn played(inputs: &mut Inputs, kind: PlayableKind, id: i64, hours_ago: i64) {
        inputs.states.insert(
            (kind, id),
            ItemState {
                played: true,
                position_ms: 0,
                last_played_at: Some(NOW - hours_ago * HOUR),
            },
        );
    }

    fn ids(items: &[ExpectedItem]) -> Vec<i64> {
        items.iter().map(|i| i.id).collect()
    }

    #[test]
    fn rolling_starts_at_the_beginning() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(3));
        let set = expected_set(&inputs);
        assert_eq!(ids(&set), vec![11, 12, 13]);
        assert!(
            set.iter()
                .all(|i| i.profile == Profile::Mobile && i.source == Source::Rolling)
        );
        assert_eq!(set[0].audio_language.as_deref(), Some("eng"));
    }

    #[test]
    fn rolling_refills_after_watching_and_keeps_watched_24h() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(2));
        played(&mut inputs, PlayableKind::Episode, 11, 30); // past the window
        played(&mut inputs, PlayableKind::Episode, 12, 2); // still kept
        let set = expected_set(&inputs);
        assert_eq!(ids(&set), vec![13, 14, 12]);
        assert!(set[2].watched);
        assert_eq!(
            set[2].keep_until,
            Some(NOW - 2 * HOUR + KEEP_AFTER_WATCHED_SECS)
        );
    }

    #[test]
    fn rolling_crosses_seasons_and_specials_come_last() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(4));
        for (id, h) in [(11, 50), (12, 49), (13, 48), (14, 47)] {
            played(&mut inputs, PlayableKind::Episode, id, h);
        }
        assert_eq!(ids(&expected_set(&inputs)), vec![21, 22, 1]);
    }

    #[test]
    fn rolling_resumes_at_an_episode_in_progress() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(2));
        played(&mut inputs, PlayableKind::Episode, 11, 40);
        inputs.states.insert(
            (PlayableKind::Episode, 12),
            ItemState {
                played: false,
                position_ms: 600_000,
                last_played_at: Some(NOW - HOUR),
            },
        );
        assert_eq!(ids(&expected_set(&inputs)), vec![12, 13]);
    }

    #[test]
    fn rolling_follows_the_last_watched_even_after_skipping() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(2));
        played(&mut inputs, PlayableKind::Episode, 14, 30); // jumped to S1E4
        assert_eq!(ids(&expected_set(&inputs)), vec![21, 22]);
    }

    #[test]
    fn rolling_at_the_end_of_the_series_has_nothing_left() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(3));
        played(&mut inputs, PlayableKind::Episode, 1, 30);
        assert!(expected_set(&inputs).is_empty());
    }

    #[test]
    fn season_downloads_once_and_never_moves_on() {
        let mut inputs = base();
        inputs.subscriptions.push(Subscription {
            series_id: 1,
            mode: Mode::Season { number: 2 },
            profile: Some(Profile::Saver),
            audio_language: None,
        });
        assert_eq!(ids(&expected_set(&inputs)), vec![21, 22]);
        played(&mut inputs, PlayableKind::Episode, 21, 1);
        played(&mut inputs, PlayableKind::Episode, 22, 30);
        let set = expected_set(&inputs);
        assert_eq!(
            ids(&set),
            vec![21],
            "S2E2 is past its window, S3 is never added"
        );
        assert_eq!(set[0].profile, Profile::Saver);
    }

    #[test]
    fn watchlist_only_with_automatic_downloads() {
        let mut inputs = base();
        inputs.watchlist = vec![
            WatchlistEntry {
                movie_id: 7,
                audio_language: Some("fre".into()),
                added_at: 20,
            },
            WatchlistEntry {
                movie_id: 5,
                audio_language: None,
                added_at: 10,
            },
        ];
        assert!(expected_set(&inputs).is_empty(), "auto off");
        inputs.auto_download = true;
        let set = expected_set(&inputs);
        assert_eq!(ids(&set), vec![5, 7], "oldest additions first");
        assert_eq!(set[0].source, Source::Watchlist);
        assert_eq!(set[1].audio_language.as_deref(), Some("fre"));
    }

    #[test]
    fn watched_watchlist_movie_stays_24h_then_leaves() {
        let mut inputs = base();
        inputs.auto_download = true;
        inputs.watchlist = vec![WatchlistEntry {
            movie_id: 5,
            audio_language: None,
            added_at: 0,
        }];
        played(&mut inputs, PlayableKind::Movie, 5, 23);
        assert!(expected_set(&inputs)[0].watched);
        played(&mut inputs, PlayableKind::Movie, 5, 25);
        assert!(expected_set(&inputs).is_empty());
    }

    #[test]
    fn manual_downloads_ignore_the_auto_switch() {
        let mut inputs = base();
        inputs.manual.push(ManualEntry {
            kind: PlayableKind::Movie,
            id: 9,
            profile: Some(Profile::Max),
            audio_language: None,
        });
        let set = expected_set(&inputs);
        assert_eq!(ids(&set), vec![9]);
        assert_eq!(set[0].profile, Profile::Max);
    }

    #[test]
    fn duplicates_keep_the_highest_priority_source() {
        let mut inputs = base();
        inputs.subscriptions.push(rolling(1));
        inputs.manual.push(ManualEntry {
            kind: PlayableKind::Episode,
            id: 11,
            profile: Some(Profile::Max),
            audio_language: None,
        });
        let set = expected_set(&inputs);
        assert_eq!(set.len(), 1);
        assert_eq!(set[0].source, Source::Rolling);
        assert_eq!(set[0].profile, Profile::Mobile);
    }

    #[test]
    fn priority_order_and_watched_items_last() {
        let mut inputs = base();
        inputs.auto_download = true;
        inputs.subscriptions.push(rolling(1));
        inputs.watchlist = vec![WatchlistEntry {
            movie_id: 5,
            audio_language: None,
            added_at: 0,
        }];
        inputs.manual.push(ManualEntry {
            kind: PlayableKind::Movie,
            id: 9,
            profile: None,
            audio_language: None,
        });
        played(&mut inputs, PlayableKind::Movie, 9, 3); // watched 3 h ago
        played(&mut inputs, PlayableKind::Movie, 5, 1); // watched 1 h ago
        let set = expected_set(&inputs);
        assert_eq!(
            ids(&set),
            vec![11, 5, 9],
            "unwatched first; then most recently watched"
        );
        assert_eq!(set.iter().filter(|i| i.watched).count(), 2);
    }

    #[test]
    fn unknown_series_is_ignored() {
        let mut inputs = base();
        inputs.subscriptions.push(Subscription {
            series_id: 404,
            ..rolling(3)
        });
        assert!(expected_set(&inputs).is_empty());
    }

    #[test]
    fn device_without_profile_defaults_to_max() {
        let mut inputs = base();
        inputs.device_profile = None;
        inputs.subscriptions.push(rolling(1));
        assert_eq!(expected_set(&inputs)[0].profile, Profile::Max);
    }
}
