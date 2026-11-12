mod common;

use std::{path::Path, process::Command, time::Duration};

use axum::http::{Method, StatusCode};
use common::{TestApp, test_config};
use serde_json::{Value, json};

struct Fixture {
    app: TestApp,
    admin: String,
    media: tempfile::TempDir,
    _cache: tempfile::TempDir,
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
        && Command::new("ffprobe").arg("-version").output().is_ok()
}

fn touch(root: &Path, relative: &str) {
    let full = root.join(relative);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, b"fake").unwrap();
}

/// A real 4 s video. `container` is the file extension (mp4, mkv).
fn make_video(root: &Path, relative: &str, audio_codec: &str) {
    let full = root.join(relative);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=4:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=4",
            "-c:v",
            "libx264",
            "-g",
            "25",
            "-c:a",
            audio_codec,
            "-metadata:s:a:0",
            "language=eng",
            "-shortest",
        ])
        .arg(&full)
        .status()
        .unwrap();
    assert!(status.success());
}

async fn fixture() -> Fixture {
    let media = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut config = test_config();
    config.media_dir = media.path().to_path_buf();
    config.cache_dir = cache.path().to_path_buf();
    let app = TestApp::with_config(config).await;
    let (admin, _) = app.ready_admin().await;
    Fixture {
        app,
        admin,
        media,
        _cache: cache,
    }
}

impl Fixture {
    async fn library(&self, kind: &str, path: &str) -> i64 {
        let (status, body) = self
            .app
            .post(
                "/api/admin/libraries",
                Some(&self.admin),
                json!({ "name": path, "kind": kind, "path": path }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id = body["id"].as_i64().unwrap();
        for _ in 0..1000 {
            if !self.app.state.scans.is_running(id) {
                return id;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("scan did not finish");
    }

    async fn get(&self, uri: &str) -> Value {
        let (status, body) = self.app.get(uri, Some(&self.admin)).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body
    }

    async fn progress(&self, kind: &str, id: i64, position_ms: i64) -> Value {
        let (status, body) = self
            .app
            .post(
                "/api/progress",
                Some(&self.admin),
                json!({ "kind": kind, "id": id, "position_ms": position_ms }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Pretends every file lasts an hour (fake files can't be probed).
    async fn set_durations(&self) {
        sqlx::query("UPDATE media_files SET duration_ms = 3600000")
            .execute(&self.app.state.db)
            .await
            .unwrap();
    }

    async fn episodes(&self, series_id: i64, season: i64) -> Vec<Value> {
        self.get(&format!("/api/series/{series_id}/seasons/{season}"))
            .await
            .as_array()
            .unwrap()
            .clone()
    }
}

fn browser_caps() -> Value {
    json!({
        "containers": ["mp4", "webm"],
        "video_codecs": ["h264", "vp9", "av1"],
        "audio_codecs": ["aac", "mp3", "opus"],
        "max_height": 2160,
        "max_audio_channels": 6
    })
}

// ---- Progress and watched state -----------------------------------------------------------

#[tokio::test]
async fn progress_resume_and_played_at_90_percent() {
    let f = fixture().await;
    touch(f.media.path(), "Films/Heat (1995)/Heat.mkv");
    f.library("movies", "Films").await;
    f.set_durations().await;
    let id = f.get("/api/movies").await["items"][0]["id"]
        .as_i64()
        .unwrap();

    assert_eq!(f.progress("movie", id, 10_000).await["played"], false);
    assert_eq!(
        f.get(&format!("/api/movies/{id}")).await["position_ms"],
        0,
        "too early to resume"
    );

    f.progress("movie", id, 1_200_000).await;
    let movie = f.get(&format!("/api/movies/{id}")).await;
    assert_eq!(
        (movie["position_ms"].as_i64(), movie["played"].as_bool()),
        (Some(1_200_000), Some(false))
    );
    let home = f.get("/api/home").await;
    assert_eq!(home["continue_watching"][0]["id"], id);
    assert_eq!(home["hero"]["id"], id);

    assert_eq!(f.progress("movie", id, 3_300_000).await["played"], true);
    let movie = f.get(&format!("/api/movies/{id}")).await;
    assert_eq!(
        (movie["position_ms"].as_i64(), movie["played"].as_bool()),
        (Some(0), Some(true))
    );
    assert!(
        f.get("/api/home").await["continue_watching"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn progress_on_unknown_item_is_404() {
    let f = fixture().await;
    let (status, _) = f
        .app
        .post(
            "/api/progress",
            Some(&f.admin),
            json!({ "kind": "movie", "id": 999, "position_ms": 1 }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn next_up_follows_the_series_and_skips_specials() {
    let f = fixture().await;
    for name in ["S01E01", "S01E02", "S02E01", "S00E01"] {
        let season = if name.starts_with("S00") {
            "Specials"
        } else {
            &format!("Season {}", &name[1..3])
        };
        touch(
            f.media.path(),
            &format!("Shows/Dark (2017)/{season}/Dark {name}.mkv"),
        );
    }
    f.library("series", "Shows").await;
    f.set_durations().await;
    let series_id = f.get("/api/series").await["items"][0]["id"]
        .as_i64()
        .unwrap();
    let s1 = f.episodes(series_id, 1).await;
    let s2 = f.episodes(series_id, 2).await;
    let (e1, e2, s2e1) = (
        s1[0]["id"].as_i64().unwrap(),
        s1[1]["id"].as_i64().unwrap(),
        s2[0]["id"].as_i64().unwrap(),
    );

    // Nothing watched: the play button starts S1E1, specials come last.
    assert_eq!(
        f.get(&format!("/api/series/{series_id}")).await["up_next"]["id"],
        e1
    );
    let seasons = f.get(&format!("/api/series/{series_id}")).await["seasons"].clone();
    assert_eq!(seasons[2]["season_number"], 0);

    f.progress("episode", e1, 3_500_000).await;
    let home = f.get("/api/home").await;
    assert_eq!(home["next_up"][0]["id"], e2);
    assert_eq!(home["next_up"][0]["title"], "Dark");
    assert_eq!(home["next_up"][0]["episode_number"], 2);

    f.progress("episode", e2, 600_000).await;
    assert_eq!(
        f.get(&format!("/api/series/{series_id}")).await["up_next"]["id"],
        e2,
        "in progress"
    );
    let home = f.get("/api/home").await;
    assert_eq!(home["continue_watching"][0]["id"], e2);
    assert!(home["next_up"].as_array().unwrap().is_empty());

    f.progress("episode", e2, 3_500_000).await;
    assert_eq!(
        f.get(&format!("/api/series/{series_id}")).await["up_next"]["id"],
        s2e1
    );

    let episode = f.get(&format!("/api/episodes/{e2}")).await;
    assert_eq!(episode["previous_episode_id"], e1);
    assert_eq!(episode["next_episode_id"], s2e1);
    let unplayed = f.get("/api/series").await["items"][0]["unplayed_count"].clone();
    assert_eq!(unplayed, 2);
}

#[tokio::test]
async fn an_episode_abandoned_early_stays_next_up() {
    let f = fixture().await;
    touch(
        f.media.path(),
        "Shows/Dark (2017)/Season 01/Dark S01E01.mkv",
    );
    touch(
        f.media.path(),
        "Shows/Dark (2017)/Season 01/Dark S01E02.mkv",
    );
    touch(
        f.media.path(),
        "Shows/Dark (2017)/Season 01/Dark S01E03.mkv",
    );
    f.library("series", "Shows").await;
    f.set_durations().await;
    let series_id = f.get("/api/series").await["items"][0]["id"]
        .as_i64()
        .unwrap();
    let episodes = f.episodes(series_id, 1).await;
    let (e1, e2) = (
        episodes[0]["id"].as_i64().unwrap(),
        episodes[1]["id"].as_i64().unwrap(),
    );

    f.progress("episode", e1, 3_500_000).await;
    // E2 opened for a few seconds only: too early to resume, but still next.
    f.progress("episode", e2, 5_000).await;
    assert_eq!(f.get("/api/home").await["next_up"][0]["id"], e2);
    assert_eq!(
        f.get(&format!("/api/series/{series_id}")).await["up_next"]["id"],
        e2
    );
}

#[tokio::test]
async fn mark_series_and_items_as_played() {
    let f = fixture().await;
    touch(
        f.media.path(),
        "Shows/Dark (2017)/Season 01/Dark S01E01.mkv",
    );
    touch(
        f.media.path(),
        "Shows/Dark (2017)/Season 01/Dark S01E02.mkv",
    );
    f.library("series", "Shows").await;
    let series_id = f.get("/api/series").await["items"][0]["id"]
        .as_i64()
        .unwrap();

    let (status, _) = f
        .app
        .request(
            Method::PUT,
            &format!("/api/played/series/{series_id}"),
            Some(&f.admin),
            Some(json!({ "played": true })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        f.episodes(series_id, 1)
            .await
            .iter()
            .all(|e| e["played"] == true)
    );
    assert_eq!(f.get("/api/series").await["items"][0]["unplayed_count"], 0);

    let e1 = f.episodes(series_id, 1).await[0]["id"].as_i64().unwrap();
    f.app
        .request(
            Method::PUT,
            &format!("/api/played/episode/{e1}"),
            Some(&f.admin),
            Some(json!({ "played": false })),
        )
        .await;
    assert_eq!(f.episodes(series_id, 1).await[0]["played"], false);

    let (status, _) = f
        .app
        .request(
            Method::PUT,
            "/api/played/movie/999",
            Some(&f.admin),
            Some(json!({ "played": true })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn progress_is_per_user() {
    let f = fixture().await;
    touch(f.media.path(), "Films/Heat (1995)/Heat.mkv");
    f.library("movies", "Films").await;
    let id = f.get("/api/movies").await["items"][0]["id"]
        .as_i64()
        .unwrap();
    f.progress("movie", id, 900_000).await;
    let alice = f.app.create_user(&f.admin, "alice").await;
    let (_, movie) = f.app.get(&format!("/api/movies/{id}"), Some(&alice)).await;
    assert_eq!(movie["position_ms"], 0);
}

#[tokio::test]
async fn search_matches_titles() {
    let f = fixture().await;
    touch(f.media.path(), "Films/Heat (1995)/Heat.mkv");
    touch(f.media.path(), "Films/Ronin (1998)/Ronin.mkv");
    touch(
        f.media.path(),
        "Shows/Heathers (2018)/Season 01/Heathers S01E01.mkv",
    );
    f.library("movies", "Films").await;
    f.library("series", "Shows").await;
    let results = f.get("/api/search?q=hea").await;
    assert_eq!(results["movies"].as_array().unwrap().len(), 1);
    assert_eq!(results["movies"][0]["title"], "Heat");
    assert_eq!(results["series"][0]["title"], "Heathers");
    assert!(
        f.get("/api/search?q=h").await["movies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        f.get("/api/search?q=%25").await["movies"]
            .as_array()
            .unwrap()
            .is_empty(),
        "% is literal"
    );
}

#[tokio::test]
async fn track_preferences_roundtrip_and_validation() {
    let f = fixture().await;
    let prefs = f.get("/api/me/preferences").await;
    assert_eq!(prefs["subtitle_mode"], "foreign");
    let (status, saved) = f
        .app
        .request(
            Method::PUT,
            "/api/me/preferences",
            Some(&f.admin),
            Some(json!({ "audio_language": "eng", "subtitle_language": "fre", "subtitle_mode": "always" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["subtitle_mode"], "always");
    let (status, _) = f
        .app
        .request(
            Method::PUT,
            "/api/me/preferences",
            Some(&f.admin),
            Some(json!({ "audio_language": "English", "subtitle_language": null, "subtitle_mode": "never" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---- Real playback (needs ffmpeg) ---------------------------------------------------------

#[tokio::test]
async fn mp4_plays_directly_with_range_requests() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    make_video(f.media.path(), "Films/Clip (2020)/Clip.mp4", "aac");
    f.library("movies", "Films").await;
    let id = f.get("/api/movies").await["items"][0]["id"]
        .as_i64()
        .unwrap();

    let (status, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "movie", "id": id, "capabilities": browser_caps() }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["method"], "direct");
    assert_eq!(info["hls"], false);
    assert_eq!(info["audio_tracks"][0]["language"], "eng");
    let url = info["url"].as_str().unwrap();

    let (status, headers, _) = f
        .app
        .request_with_headers(Method::GET, url, &[("range", "bytes=0-99")], None)
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers["content-length"], "100");

    // A forged token is refused.
    let (status, _) = f.app.get(&format!("{url}x"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Every level is described; a file lighter than SOS still plays directly.
    assert_eq!(info["quality"], "max");
    let levels: Vec<&str> = info["qualities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["quality"].as_str().unwrap())
        .collect();
    assert_eq!(levels, ["max", "medium", "low", "sos"]);
    assert_eq!(info["qualities"][3]["height"], 240);
    assert_eq!(info["qualities"][3]["original"], true);
    let (status, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "movie", "id": id, "capabilities": browser_caps(), "quality": "sos" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(
        (&info["method"], &info["quality"]),
        (&json!("direct"), &json!("sos"))
    );
}

#[tokio::test]
async fn mkv_is_remuxed_to_hls_and_sidecar_subtitles_become_webvtt() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    make_video(f.media.path(), "Films/Clip (2020)/Clip.mkv", "aac");
    std::fs::write(
        f.media.path().join("Films/Clip (2020)/Clip.fr.srt"),
        "1\n00:00:01,000 --> 00:00:02,500\nBonjour\n\n2\n00:00:03,000 --> 00:00:03,500\nAu revoir\n",
    )
    .unwrap();
    f.library("movies", "Films").await;
    let id = f.get("/api/movies").await["items"][0]["id"]
        .as_i64()
        .unwrap();

    let (status, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "movie", "id": id, "capabilities": browser_caps(), "start_ms": 2000 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["method"], "remux");
    assert_eq!(info["hls"], true);
    assert_eq!(info["start_ms"], 2000);
    // French subtitles chosen automatically: the admin's language is fr-FR, the audio English.
    assert_eq!(info["subtitles"][0]["external"], true);
    assert_eq!(info["subtitle"], info["subtitles"][0]["id"]);

    let playlist_url = info["url"].as_str().unwrap();
    let (status, headers, _) = f
        .app
        .request_with_headers(Method::GET, playlist_url, &[], None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/vnd.apple.mpegurl");
    let base = playlist_url.trim_end_matches("index.m3u8");
    let (status, _, _) = f
        .app
        .request_with_headers(Method::GET, &format!("{base}init.mp4"), &[], None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = f
        .app
        .request_with_headers(Method::GET, &format!("{base}ffmpeg.log"), &[], None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "only HLS files are served");

    // Subtitles are shifted by the 2 s start offset.
    let (status, vtt) = f
        .app
        .get(info["subtitle_url"].as_str().unwrap(), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let vtt = vtt.as_str().unwrap();
    assert!(vtt.starts_with("WEBVTT"), "{vtt}");
    assert!(
        vtt.contains("00:00:00.000 --> 00:00:00.500\nBonjour"),
        "{vtt}"
    );
    assert!(
        vtt.contains("00:00:01.000 --> 00:00:01.500\nAu revoir"),
        "{vtt}"
    );

    let session = info["session_id"].as_str().unwrap();
    let (status, _) = f
        .app
        .post(
            "/api/playback/stop",
            Some(&f.admin),
            json!({ "session_id": session }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = f
        .app
        .request_with_headers(Method::GET, playlist_url, &[], None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn apple_players_get_the_subtitles_inside_the_hls_stream() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    make_video(f.media.path(), "Films/Clip (2020)/Clip.mkv", "aac");
    std::fs::write(
        f.media.path().join("Films/Clip (2020)/Clip.fr.srt"),
        "1\n00:00:01,000 --> 00:00:02,500\nBonjour\n",
    )
    .unwrap();
    f.library("movies", "Films").await;
    let id = f.get("/api/movies").await["items"][0]["id"]
        .as_i64()
        .unwrap();
    let mut caps = browser_caps();
    caps["hls_subtitles"] = json!(true);

    let (status, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "movie", "id": id, "capabilities": caps }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["hls"], true);
    let master_url = info["url"].as_str().unwrap();
    assert!(master_url.ends_with("/master.m3u8"), "{master_url}");

    let (status, master) = f.app.get(master_url, None).await;
    assert_eq!(status, StatusCode::OK);
    let master = master.as_str().unwrap();
    assert!(master.contains("TYPE=SUBTITLES"), "{master}");
    assert!(master.contains("URI=\"subs.m3u8\""), "{master}");
    assert!(
        master.contains("SUBTITLES=\"subs\"\nindex.m3u8"),
        "{master}"
    );

    let base = master_url.trim_end_matches("master.m3u8");
    let (status, subs) = f.app.get(&format!("{base}subs.m3u8"), None).await;
    assert_eq!(status, StatusCode::OK);
    let subs = subs.as_str().unwrap();
    assert!(
        subs.contains(info["subtitle_url"].as_str().unwrap()),
        "{subs}"
    );
    assert!(subs.contains("#EXT-X-ENDLIST"), "{subs}");
    let (status, _, _) = f
        .app
        .request_with_headers(Method::GET, &format!("{base}index.m3u8"), &[], None)
        .await;
    assert_eq!(status, StatusCode::OK);

    // Other clients keep the plain playlist and the WebVTT beside it.
    let (_, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "movie", "id": id, "capabilities": browser_caps() }),
        )
        .await;
    assert!(info["url"].as_str().unwrap().ends_with("/index.m3u8"));
}

#[tokio::test]
async fn unsupported_audio_is_transcoded_and_choices_are_remembered_per_series() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    for e in ["S01E01", "S01E02"] {
        make_video(
            f.media.path(),
            &format!("Shows/Clip (2020)/Season 01/Clip {e}.mp4"),
            "ac3",
        );
    }
    f.library("series", "Shows").await;
    let series_id = f.get("/api/series").await["items"][0]["id"]
        .as_i64()
        .unwrap();
    let episodes = f.episodes(series_id, 1).await;
    let (e1, e2) = (
        episodes[0]["id"].as_i64().unwrap(),
        episodes[1]["id"].as_i64().unwrap(),
    );

    let (_, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "episode", "id": e1, "capabilities": browser_caps() }),
        )
        .await;
    assert_eq!(info["method"], "transcode_audio", "{info}");
    assert_eq!(info["next_episode_id"], e2);

    // The user turns subtitles off on E1 (there are none anyway): remembered for E2.
    let (_, info) = f
        .app
        .post(
            "/api/playback",
            Some(&f.admin),
            json!({ "kind": "episode", "id": e1, "capabilities": browser_caps(),
                    "audio_index": 1, "subtitle": "off", "remember": true }),
        )
        .await;
    assert_eq!(info["subtitle"], Value::Null);
    let stored: (bool,) =
        sqlx::query_as("SELECT subtitles_off FROM series_track_preferences WHERE series_id = ?")
            .bind(series_id)
            .fetch_one(&f.app.state.db)
            .await
            .unwrap();
    assert!(stored.0);
    f.app
        .state
        .transcodes
        .stop(info["session_id"].as_str().unwrap());
}
