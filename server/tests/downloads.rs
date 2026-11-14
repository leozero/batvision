mod common;

use std::{path::Path, process::Command, time::Duration};

use axum::http::{Method, StatusCode};
use batvision_server::downloads::encode::Encoders;
use common::{PASSWORD, TestApp, device, test_config};
use serde_json::{Value, json};

struct Fixture {
    app: TestApp,
    admin: String,
    /// Alice signed in on an Android phone (a download device).
    phone: String,
    phone_id: i64,
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

/// A real 3 s MKV with English then French audio.
fn make_video(root: &Path, relative: &str) {
    make_video_of(root, relative, 3);
}

fn make_video_of(root: &Path, relative: &str, seconds: u32) {
    let full = root.join(relative);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=duration={seconds}:size=640x360:rate=25"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:duration={seconds}"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=660:duration={seconds}"),
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=fre",
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
    let phone = app.create_user(&admin, "alice").await;
    let (_, devices) = app.get("/api/me/devices", Some(&phone)).await;
    let phone_id = devices[0]["id"].as_i64().unwrap();
    Fixture {
        app,
        admin,
        phone,
        phone_id,
        media,
        _cache: cache,
    }
}

impl Fixture {
    async fn library(&self, kind: &str, path: &str) {
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
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("scan did not finish");
    }

    async fn plan(&self) -> Value {
        let (status, body) = self
            .app
            .post("/api/downloads/plan", Some(&self.phone), json!({}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn first_id(&self, uri: &str) -> i64 {
        let (_, body) = self.app.get(uri, Some(&self.phone)).await;
        body["items"][0]["id"].as_i64().unwrap()
    }

    async fn set_auto(&self, auto: bool) {
        let (status, body) = self
            .app
            .request(
                Method::PUT,
                &format!("/api/downloads/devices/{}/settings", self.phone_id),
                Some(&self.phone),
                Some(
                    json!({ "auto_download": auto, "profile": "mobile", "quota_bytes": null,
                             "mobile_data": "off", "mobile_cap_bytes": null, "storage": "internal",
                         "charging_only": false }),
                ),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
}

fn ids(plan: &Value) -> Vec<i64> {
    plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn browsers_cannot_download() {
    let f = fixture().await;
    let (_, headers, _) = f
        .app
        .request_with_headers(
            Method::POST,
            "/api/auth/login",
            &[],
            Some(json!({ "username": "alice", "password": PASSWORD, "device": device("Firefox", "web") })),
        )
        .await;
    let cookie = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let (status, _, err) = f
        .app
        .request_with_headers(
            Method::POST,
            "/api/downloads/plan",
            &[("cookie", &cookie)],
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "not_a_download_device");
}

#[tokio::test]
async fn device_settings_defaults_and_ownership() {
    let f = fixture().await;
    let plan = f.plan().await;
    assert_eq!(plan["settings"]["auto_download"], false, "off by default");
    assert_eq!(
        plan["settings"]["profile"], "mobile",
        "Android defaults to the mobile profile"
    );

    assert_eq!(
        plan["settings"]["quota_bytes"], 20_000_000_000_i64,
        "20 GB on Android"
    );
    assert_eq!(
        plan["settings"]["mobile_data"], "off",
        "Wi-Fi only by default"
    );
    assert_eq!(plan["settings"]["charging_only"], false);

    f.set_auto(true).await;
    assert_eq!(f.plan().await["settings"]["auto_download"], true);

    // Mobile data mode, cap and charging rule are kept for the device's engine.
    let (status, body) = f
        .app
        .request(
            Method::PUT,
            &format!("/api/downloads/devices/{}/settings", f.phone_id),
            Some(&f.phone),
            Some(
                json!({ "auto_download": true, "profile": "mobile", "quota_bytes": 5_000_000_000_i64,
                         "mobile_data": "all", "mobile_cap_bytes": 2_000_000_000_i64,
                         "storage": "sd", "charging_only": true }),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let settings = &f.plan().await["settings"];
    assert_eq!(settings["mobile_data"], "all");
    assert_eq!(settings["mobile_cap_bytes"], 2_000_000_000_i64);
    assert_eq!(settings["storage"], "sd");
    assert_eq!(settings["charging_only"], true);

    // Someone else's device can't be configured.
    let (status, _) = f
        .app
        .request(
            Method::PUT,
            &format!("/api/downloads/devices/{}/settings", f.phone_id),
            Some(&f.admin),
            Some(
                json!({ "auto_download": true, "profile": "max", "quota_bytes": null,
                         "mobile_data": "off", "mobile_cap_bytes": null, "storage": "internal",
                         "charging_only": false }),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn watchlist_downloads_only_with_auto_and_leaves_once_watched() {
    let f = fixture().await;
    touch(f.media.path(), "Movies/Heat (1995)/Heat.mkv");
    f.library("movies", "Movies").await;
    let movie = f.first_id("/api/movies").await;

    let (status, _) = f
        .app
        .request(
            Method::PUT,
            &format!("/api/watchlist/{movie}"),
            Some(&f.phone),
            Some(json!({ "audio_language": "fr" })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = f.app.get("/api/watchlist", Some(&f.phone)).await;
    assert_eq!(list[0]["audio_language"], "fre", "normalized");
    let (_, detail) = f
        .app
        .get(&format!("/api/movies/{movie}"), Some(&f.phone))
        .await;
    assert_eq!(detail["in_watchlist"], true);

    assert!(
        ids(&f.plan().await).is_empty(),
        "automatic downloads are off"
    );
    f.set_auto(true).await;
    let plan = f.plan().await;
    assert_eq!(ids(&plan), vec![movie]);
    assert_eq!(plan["items"][0]["source"], "watchlist");
    assert_eq!(plan["items"][0]["title"], "Heat");
    assert_eq!(
        plan["items"][0]["status"], "unavailable",
        "fake file can't be encoded"
    );

    // Watched long ago (offline, synced later): it leaves the device.
    let (status, res) = f
        .app
        .post(
            "/api/progress/batch",
            Some(&f.phone),
            json!([{ "kind": "movie", "id": movie, "position_ms": 0, "played": true,
                     "at": chrono_now() - 2 * 86_400 }]),
        )
        .await;
    assert_eq!((status, res["applied"].as_i64()), (StatusCode::OK, Some(1)));
    assert!(ids(&f.plan().await).is_empty());
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[tokio::test]
async fn rolling_subscription_follows_progress() {
    let f = fixture().await;
    for e in ["01", "02", "03", "04"] {
        touch(
            f.media.path(),
            &format!("Shows/Dark (2017)/Season 01/Dark S01E{e}.mkv"),
        );
    }
    f.library("series", "Shows").await;
    let series = f.first_id("/api/series").await;
    let (_, episodes) = f
        .app
        .get(&format!("/api/series/{series}/seasons/1"), Some(&f.phone))
        .await;
    let ep: Vec<i64> = episodes
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();

    let (status, err) = f
        .app
        .post(
            "/api/downloads/subscriptions",
            Some(&f.phone),
            json!({ "device_id": f.phone_id, "series_id": series, "mode": "rolling", "count": 99 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{err}");

    let (status, _) = f
        .app
        .post(
            "/api/downloads/subscriptions",
            Some(&f.phone),
            json!({ "device_id": f.phone_id, "series_id": series, "mode": "rolling", "count": 2,
                      "audio_language": "eng" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        ids(&f.plan().await),
        vec![ep[0], ep[1]],
        "works even with auto downloads off"
    );

    // E1 watched offline an hour ago: E2, E3 next, E1 kept for 24 h at the end.
    f.app
        .post("/api/progress/batch", Some(&f.phone),
              json!([{ "kind": "episode", "id": ep[0], "position_ms": 0, "played": true, "at": chrono_now() - 3600 }]))
        .await;
    let plan = f.plan().await;
    assert_eq!(ids(&plan), vec![ep[1], ep[2], ep[0]]);
    assert_eq!(plan["items"][2]["watched"], true);

    // A new rolling subscription replaces the old one.
    f.app
        .post(
            "/api/downloads/subscriptions",
            Some(&f.phone),
            json!({ "device_id": f.phone_id, "series_id": series, "mode": "rolling", "count": 1 }),
        )
        .await;
    let (_, overview) = f.app.get("/api/downloads/overview", Some(&f.phone)).await;
    assert_eq!(
        overview["devices"][0]["subscriptions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        overview["devices"][0]["subscriptions"][0]["series_title"],
        "Dark"
    );

    let sub = overview["devices"][0]["subscriptions"][0]["id"].clone();
    let (status, _) = f
        .app
        .request(
            Method::DELETE,
            &format!("/api/downloads/subscriptions/{sub}"),
            Some(&f.phone),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let remaining = ids(&f.plan().await);
    assert_eq!(remaining, vec![] as Vec<i64>);
}

async fn dark_episodes(f: &Fixture) -> (i64, Vec<i64>) {
    for e in ["01", "02", "03", "04"] {
        touch(
            f.media.path(),
            &format!("Shows/Dark (2017)/Season 01/Dark S01E{e}.mkv"),
        );
    }
    f.library("series", "Shows").await;
    let series = f.first_id("/api/series").await;
    let (_, episodes) = f
        .app
        .get(&format!("/api/series/{series}/seasons/1"), Some(&f.phone))
        .await;
    let ep = episodes
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    (series, ep)
}

#[tokio::test]
async fn watchlist_series_downloads_the_next_episodes() {
    let f = fixture().await;
    let (series, ep) = dark_episodes(&f).await;
    touch(f.media.path(), "Movies/Heat (1995)/Heat.mkv");
    f.library("movies", "Movies").await;
    let movie = f.first_id("/api/movies").await;

    let (status, _) = f
        .app
        .request(
            Method::PUT,
            &format!("/api/watchlist/series/{series}"),
            Some(&f.phone),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = f
        .app
        .request(
            Method::PUT,
            "/api/watchlist/series/9999",
            Some(&f.phone),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    f.app
        .request(
            Method::PUT,
            &format!("/api/watchlist/{movie}"),
            Some(&f.phone),
            Some(json!({})),
        )
        .await;
    let (_, detail) = f
        .app
        .get(&format!("/api/series/{series}"), Some(&f.phone))
        .await;
    assert_eq!(detail["in_watchlist"], true);

    // Most recent additions first, movies and series together.
    let (_, list) = f.app.get("/api/watchlist", Some(&f.phone)).await;
    let kinds: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(kinds.contains(&"series") && kinds.contains(&"movie"));
    let dark = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "series")
        .unwrap();
    assert_eq!(
        (dark["title"].as_str(), dark["played"].as_bool()),
        (Some("Dark"), Some(false))
    );

    assert!(
        ids(&f.plan().await).is_empty(),
        "automatic downloads are off"
    );
    f.set_auto(true).await;
    // Both were added in the same second: make the series the older one.
    sqlx::query("UPDATE watchlist_series SET added_at = added_at - 10")
        .execute(&f.app.state.db)
        .await
        .unwrap();
    let plan = f.plan().await;
    assert_eq!(
        ids(&plan),
        vec![ep[0], ep[1], movie],
        "2 episodes by default, by date added"
    );
    assert_eq!(plan["items"][0]["source"], "watchlist");

    // The account setting changes how many.
    let (_, prefs) = f.app.get("/api/me/preferences", Some(&f.phone)).await;
    assert_eq!(prefs["watchlist_episodes"], 2);
    let (status, _) = f
        .app
        .request(
            Method::PUT,
            "/api/me/preferences",
            Some(&f.phone),
            Some(json!({ "audio_language": null, "subtitle_language": null,
                         "subtitle_mode": "foreign", "watchlist_episodes": 11 })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, prefs) = f
        .app
        .request(
            Method::PUT,
            "/api/me/preferences",
            Some(&f.phone),
            Some(json!({ "audio_language": null, "subtitle_language": null,
                         "subtitle_mode": "foreign", "watchlist_episodes": 3 })),
        )
        .await;
    assert_eq!(
        (status, &prefs["watchlist_episodes"]),
        (StatusCode::OK, &json!(3))
    );
    // Older clients don't send it: unchanged.
    let (_, prefs) = f
        .app
        .request(
            Method::PUT,
            "/api/me/preferences",
            Some(&f.phone),
            Some(json!({ "audio_language": null, "subtitle_language": null, "subtitle_mode": "foreign" })),
        )
        .await;
    assert_eq!(prefs["watchlist_episodes"], 3);
    assert_eq!(ids(&f.plan().await), vec![ep[0], ep[1], ep[2], movie]);

    // E1 watched: the window moves on.
    f.app
        .post(
            "/api/progress/batch",
            Some(&f.phone),
            json!([{ "kind": "episode", "id": ep[0], "position_ms": 0, "played": true,
                       "at": chrono_now() - 2 * 86_400 }]),
        )
        .await;
    assert_eq!(ids(&f.plan().await), vec![ep[1], ep[2], ep[3], movie]);

    let (status, _) = f
        .app
        .request(
            Method::DELETE,
            &format!("/api/watchlist/series/{series}"),
            Some(&f.phone),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(ids(&f.plan().await), vec![movie]);
}

async fn cancel(f: &Fixture, kind: &str, id: i64) -> Vec<String> {
    let (status, body) = f
        .app
        .post(
            "/api/downloads/cancel",
            Some(&f.phone),
            json!({ "kind": kind, "id": id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["removed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn cancel_removes_what_asks_for_the_item() {
    let f = fixture().await;
    let (series, ep) = dark_episodes(&f).await;
    touch(f.media.path(), "Movies/Heat (1995)/Heat.mkv");
    f.library("movies", "Movies").await;
    let movie = f.first_id("/api/movies").await;
    f.set_auto(true).await;

    // A movie both in the watchlist and requested by hand: both go.
    f.app
        .request(
            Method::PUT,
            &format!("/api/watchlist/{movie}"),
            Some(&f.phone),
            Some(json!({})),
        )
        .await;
    f.app
        .post(
            "/api/downloads/manual",
            Some(&f.phone),
            json!({ "device_id": f.phone_id, "kind": "movie", "id": movie }),
        )
        .await;
    assert_eq!(ids(&f.plan().await), vec![movie]);
    assert_eq!(
        cancel(&f, "movie", movie).await,
        vec!["manual", "watchlist"]
    );
    assert!(ids(&f.plan().await).is_empty());
    let (_, list) = f.app.get("/api/watchlist", Some(&f.phone)).await;
    assert_eq!(list, json!([]));

    // An episode of a rolling subscription: the subscription goes.
    f.app
        .post(
            "/api/downloads/subscriptions",
            Some(&f.phone),
            json!({ "device_id": f.phone_id, "series_id": series, "mode": "rolling", "count": 2 }),
        )
        .await;
    assert_eq!(ids(&f.plan().await), vec![ep[0], ep[1]]);
    assert_eq!(cancel(&f, "episode", ep[1]).await, vec!["rolling"]);
    let (_, overview) = f.app.get("/api/downloads/overview", Some(&f.phone)).await;
    assert_eq!(overview["devices"][0]["subscriptions"], json!([]));

    // A season subscription and the watchlist series together.
    f.app
        .post(
            "/api/downloads/subscriptions",
            Some(&f.phone),
            json!({ "device_id": f.phone_id, "series_id": series, "mode": "season", "season_number": 1 }),
        )
        .await;
    f.app
        .request(
            Method::PUT,
            &format!("/api/watchlist/series/{series}"),
            Some(&f.phone),
            None,
        )
        .await;
    assert_eq!(
        cancel(&f, "episode", ep[0]).await,
        vec!["season", "watchlist"]
    );
    assert!(ids(&f.plan().await).is_empty());

    // Nothing asks for it any more: nothing to remove.
    assert!(cancel(&f, "episode", ep[0]).await.is_empty());
}

/// A long movie in the phone's watchlist, encoding for it. Returns (movie, variant).
async fn long_encode(f: &Fixture) -> (i64, i64) {
    // Long enough to still be encoding when the test stops it.
    make_video_of(f.media.path(), "Movies/Long (2020)/Long (2020).mkv", 600);
    f.library("movies", "Movies").await;
    let movie = f.first_id("/api/movies").await;
    f.app
        .state
        .encoder
        .clone()
        .start(Encoders::default)
        .await
        .unwrap();
    f.set_auto(true).await;
    f.app
        .request(
            Method::PUT,
            &format!("/api/watchlist/{movie}"),
            Some(&f.phone),
            Some(json!({})),
        )
        .await;
    let (status, requested) = f
        .app
        .post(
            "/api/downloads/request",
            Some(&f.phone),
            json!({ "kind": "movie", "id": movie }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{requested}");
    let variant = requested["variant_id"].as_i64().unwrap();
    for _ in 0..100 {
        if f.app.state.encoder.is_encoding(variant) {
            return (movie, variant);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the encode didn't start");
}

/// ffmpeg is stopped, not left to finish a 10-minute encode, and the variant goes.
async fn assert_stopped(f: &Fixture, variant: i64) {
    for _ in 0..50 {
        if !f.app.state.encoder.is_encoding(variant) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!f.app.state.encoder.is_encoding(variant));
    assert!(!f.app.state.encoder.dir(variant).exists());
    let (_, queue) = f.app.get("/api/admin/encodes", Some(&f.admin)).await;
    assert!(queue["variants"].as_array().unwrap().is_empty(), "{queue}");
}

#[tokio::test]
async fn cancelling_stops_an_encode_nobody_waits_for() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    let (movie, variant) = long_encode(&f).await;
    cancel(&f, "movie", movie).await;
    assert_stopped(&f, variant).await;
}

#[tokio::test]
async fn an_item_leaving_the_plan_stops_its_encode() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    let (movie, variant) = long_encode(&f).await;
    // Removed from the watchlist on the web: the phone's next plan withdraws its request.
    f.app
        .request(
            Method::DELETE,
            &format!("/api/watchlist/{movie}"),
            Some(&f.phone),
            None,
        )
        .await;
    assert!(ids(&f.plan().await).is_empty());
    assert_stopped(&f, variant).await;
}

#[tokio::test]
async fn season_subscription_needs_an_existing_season() {
    let f = fixture().await;
    touch(
        f.media.path(),
        "Shows/Dark (2017)/Season 01/Dark S01E01.mkv",
    );
    f.library("series", "Shows").await;
    let series = f.first_id("/api/series").await;
    let (status, _) = f
        .app
        .post("/api/downloads/subscriptions", Some(&f.phone),
              json!({ "device_id": f.phone_id, "series_id": series, "mode": "season", "season_number": 9 }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn offline_progress_never_overwrites_a_more_recent_watch() {
    let f = fixture().await;
    touch(f.media.path(), "Movies/Heat (1995)/Heat.mkv");
    f.library("movies", "Movies").await;
    let movie = f.first_id("/api/movies").await;
    sqlx::query("UPDATE media_files SET duration_ms = 3600000")
        .execute(&f.app.state.db)
        .await
        .unwrap();
    // Watched on the web just now…
    f.app
        .post(
            "/api/progress",
            Some(&f.phone),
            json!({ "kind": "movie", "id": movie, "position_ms": 1_800_000 }),
        )
        .await;
    // …then an older offline record arrives.
    let (_, res) = f
        .app
        .post("/api/progress/batch", Some(&f.phone),
              json!([{ "kind": "movie", "id": movie, "position_ms": 600_000, "played": false, "at": chrono_now() - 3600 },
                     { "kind": "movie", "id": 999_999, "position_ms": 1, "played": false, "at": chrono_now() }]))
        .await;
    assert_eq!(res["applied"], 0);
    let (_, detail) = f
        .app
        .get(&format!("/api/movies/{movie}"), Some(&f.phone))
        .await;
    assert_eq!(detail["position_ms"], 1_800_000);
}

#[tokio::test]
async fn request_encode_download_confirm_and_cache_cleanup() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    make_video(f.media.path(), "Movies/Clip (2020)/Clip (2020).mkv");
    std::fs::write(
        f.media.path().join("Movies/Clip (2020)/Clip (2020).fr.srt"),
        "1\n00:00:00,500 --> 00:00:01,500\nBonjour\n",
    )
    .unwrap();
    f.library("movies", "Movies").await;
    let movie = f.first_id("/api/movies").await;
    f.app
        .state
        .encoder
        .clone()
        .start(Encoders::default)
        .await
        .unwrap();

    // Watchlist in French, automatic downloads on, Max profile.
    f.app
        .request(
            Method::PUT,
            &format!("/api/watchlist/{movie}"),
            Some(&f.phone),
            Some(json!({ "audio_language": "fre" })),
        )
        .await;
    f.app
        .request(
            Method::PUT,
            &format!("/api/downloads/devices/{}/settings", f.phone_id),
            Some(&f.phone),
            Some(
                json!({ "auto_download": true, "profile": "max", "quota_bytes": null,
                         "mobile_data": "off", "mobile_cap_bytes": null, "storage": "internal",
                         "charging_only": false }),
            ),
        )
        .await;
    assert_eq!(
        f.plan().await["items"][0]["status"],
        "not_requested",
        "no encoding before a request"
    );
    let (_, admin_queue) = f.app.get("/api/admin/encodes", Some(&f.admin)).await;
    assert!(admin_queue["variants"].as_array().unwrap().is_empty());

    let (status, requested) = f
        .app
        .post(
            "/api/downloads/request",
            Some(&f.phone),
            json!({ "kind": "movie", "id": movie }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{requested}");
    let variant = requested["variant_id"].as_i64().unwrap();

    let mut item = Value::Null;
    for _ in 0..300 {
        item = f.plan().await["items"][0].clone();
        if item["status"] == "ready" || item["status"] == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(item["status"], "ready", "{item}");
    let url = item["url"].as_str().unwrap().to_owned();
    let size = item["size_bytes"].as_i64().unwrap();
    assert!(size > 1000);

    // Resumable download.
    let (status, headers, _) = f
        .app
        .request_with_headers(Method::GET, &url, &[("range", "bytes=0-99")], None)
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers["content-length"], "100");

    // The plan says which track plays first; the MP4 holds both, as AAC, in the source's order.
    assert_eq!(item["audio_language"], "fre");
    let out = f.app.state.encoder.dir(variant).join("video.mp4");
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name:stream_tags=language",
            "-of",
            "json",
        ])
        .arg(&out)
        .output()
        .unwrap();
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = probe["streams"].as_array().unwrap();
    let audio: Vec<&Value> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .collect();
    assert_eq!(audio.len(), 2);
    assert!(audio.iter().all(|a| a["codec_name"] == "aac"));
    assert_eq!(audio[0]["tags"]["language"], "eng");
    assert_eq!(audio[1]["tags"]["language"], "fre");
    assert!(
        streams
            .iter()
            .any(|s| s["codec_type"] == "video" && s["codec_name"] == "h264"),
        "Max copies video"
    );

    // The sidecar subtitle ships as WebVTT.
    assert_eq!(item["subtitles"][0]["language"], "fre");
    let (status, vtt) = f
        .app
        .get(item["subtitles"][0]["url"].as_str().unwrap(), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(vtt.as_str().unwrap().contains("Bonjour"));

    // Confirmed: shown as downloaded, the cache copy is gone.
    let (status, _) = f
        .app
        .post(
            "/api/downloads/confirm",
            Some(&f.phone),
            json!({ "variant_id": variant }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!out.exists());
    assert_eq!(f.plan().await["items"][0]["status"], "downloaded");
    let (status, _, _) = f
        .app
        .request_with_headers(Method::GET, &url, &[], None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
