//! Several files for one movie or episode (D102): the versions a user picks from.

mod common;

use std::{path::Path, process::Command, time::Duration};

use axum::http::StatusCode;
use common::{TestApp, test_config};
use serde_json::{Value, json};

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
        && Command::new("ffprobe").arg("-version").output().is_ok()
}

/// A real H.264 + AAC video of `seconds` at `size` (`WxH`), with English audio.
fn make_video(root: &Path, relative: &str, size: &str, seconds: u32) {
    let full = root.join(relative);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("testsrc2=duration={seconds}:size={size}:rate=25"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!("sine=duration={seconds}"))
        .args([
            "-c:v",
            "libx264",
            "-g",
            "25",
            "-c:a",
            "aac",
            "-metadata:s:a:0",
            "language=eng",
            "-shortest",
        ])
        .arg(&full)
        .status()
        .unwrap();
    assert!(status.success());
}

struct Fixture {
    app: TestApp,
    admin: String,
    media: tempfile::TempDir,
    cache: tempfile::TempDir,
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
        cache,
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

    async fn get(&self, token: &str, uri: &str) -> Value {
        let (status, body) = self.app.get(uri, Some(token)).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body
    }

    async fn play(&self, token: &str, body: Value) -> (StatusCode, Value) {
        self.app.post("/api/playback", Some(token), body).await
    }
}

fn caps() -> Value {
    json!({
        "containers": ["mp4", "webm"],
        "video_codecs": ["h264"],
        "audio_codecs": ["aac"],
        "max_height": 2160,
        "max_audio_channels": 6
    })
}

/// Heat with a 720p copy (8 s, the largest) and an SD one (4 s).
async fn two_versions() -> (Fixture, i64, i64, i64) {
    let f = fixture().await;
    make_video(
        f.media.path(),
        "Films/Heat (1995)/Heat (1995) 720p.mp4",
        "1280x720",
        8,
    );
    make_video(
        f.media.path(),
        "Films/Heat (1995)/Heat (1995) SD.mp4",
        "320x240",
        4,
    );
    f.library("movies", "Films").await;
    let movies = f.get(&f.admin, "/api/movies").await;
    assert_eq!(movies["total"], 1, "one movie: {movies}");
    let id = movies["items"][0]["id"].as_i64().unwrap();
    let detail = f.get(&f.admin, &format!("/api/movies/{id}")).await;
    let versions = detail["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2, "{detail}");
    let hd = versions[0]["id"].as_i64().unwrap();
    let sd = versions[1]["id"].as_i64().unwrap();
    (f, id, hd, sd)
}

#[tokio::test]
async fn the_detail_lists_the_versions_largest_first() {
    if !ffmpeg_available() {
        return;
    }
    let (f, id, hd, _) = two_versions().await;
    let detail = f.get(&f.admin, &format!("/api/movies/{id}")).await;
    let v = &detail["versions"];
    assert_eq!(v[0]["label"], "720p · H.264");
    assert_eq!(v[0]["file_name"], "Heat (1995) 720p.mp4");
    assert_eq!(
        (&v[0]["width"], &v[0]["height"]),
        (&json!(1280), &json!(720))
    );
    assert_eq!(v[0]["video_codec"], "h264");
    assert_eq!(v[0]["hdr"], false);
    assert_eq!(v[0]["audio_languages"], json!(["eng"]));
    assert_eq!(
        (&v[0]["default"], &v[0]["selected"]),
        (&json!(true), &json!(true))
    );
    assert_eq!(v[1]["label"], "240p · H.264");
    assert_eq!(
        (&v[1]["default"], &v[1]["selected"]),
        (&json!(false), &json!(false))
    );
    assert!(v[0]["size_bytes"].as_i64() > v[1]["size_bytes"].as_i64());
    assert_eq!(detail["file"]["id"], hd);
}

#[tokio::test]
async fn a_chosen_version_is_played_and_remembered_per_user() {
    if !ffmpeg_available() {
        return;
    }
    let (f, id, hd, sd) = two_versions().await;
    let user = f.app.create_user(&f.admin, "bob").await;

    // By default, the largest.
    let (status, info) = f
        .play(
            &f.admin,
            json!({ "kind": "movie", "id": id, "capabilities": caps() }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["media_file_id"], hd);
    assert_eq!(info["versions"].as_array().unwrap().len(), 2);

    // Picking the SD copy plays it…
    let (status, info) = f
        .play(
            &f.admin,
            json!({ "kind": "movie", "id": id, "capabilities": caps(), "media_file_id": sd }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["media_file_id"], sd);
    let duration = info["duration_ms"].as_i64().unwrap();
    assert!((3_500..4_500).contains(&duration), "{duration}");
    assert_eq!(info["versions"][1]["selected"], true);

    // …and again next time, on the detail page too, for this user only.
    let (_, info) = f
        .play(
            &f.admin,
            json!({ "kind": "movie", "id": id, "capabilities": caps() }),
        )
        .await;
    assert_eq!(info["media_file_id"], sd);
    let detail = f.get(&f.admin, &format!("/api/movies/{id}")).await;
    assert_eq!(detail["file"]["id"], sd);
    assert_eq!(detail["versions"][1]["selected"], true);
    let theirs = f.get(&user, &format!("/api/movies/{id}")).await;
    assert_eq!(theirs["file"]["id"], hd);
    let (_, info) = f
        .play(
            &user,
            json!({ "kind": "movie", "id": id, "capabilities": caps() }),
        )
        .await;
    assert_eq!(info["media_file_id"], hd);

    // Progress is measured against the version played: 3.7 s of 4 s is the end.
    let (status, body) = f
        .app
        .post(
            "/api/progress",
            Some(&f.admin),
            json!({ "kind": "movie", "id": id, "position_ms": 3_700 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["played"], true, "{body}");
    let (_, body) = f
        .app
        .post(
            "/api/progress",
            Some(&user),
            json!({ "kind": "movie", "id": id, "position_ms": 3_700 }),
        )
        .await;
    assert_eq!(body["played"], false, "3.7 s of 8 s: {body}");

    // Picking the default again forgets the choice.
    let (_, info) = f
        .play(
            &f.admin,
            json!({ "kind": "movie", "id": id, "capabilities": caps(), "media_file_id": hd }),
        )
        .await;
    assert_eq!(info["media_file_id"], hd);
    let choices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM version_choices")
        .fetch_one(&f.app.state.db)
        .await
        .unwrap();
    assert_eq!(choices, 0);
}

#[tokio::test]
async fn a_version_must_belong_to_the_item() {
    if !ffmpeg_available() {
        return;
    }
    let (f, id, _, _) = two_versions().await;
    make_video(
        f.media.path(),
        "Other/Ronin (1998)/Ronin (1998).mp4",
        "320x240",
        2,
    );
    f.library("movies", "Other").await;
    let other: i64 = sqlx::query_scalar("SELECT id FROM media_files WHERE path LIKE '%Ronin%'")
        .fetch_one(&f.app.state.db)
        .await
        .unwrap();
    let (status, body) = f
        .play(
            &f.admin,
            json!({ "kind": "movie", "id": id, "capabilities": caps(), "media_file_id": other }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "unknown_version", "{body}");
    let (status, _) = f
        .app
        .get(
            &format!("/api/playback/trickplay?kind=movie&id={id}&media_file_id={other}"),
            Some(&f.admin),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thumbnails_follow_the_version_played() {
    if !ffmpeg_available() {
        return;
    }
    let (f, id, hd, sd) = two_versions().await;
    f.app.state.trickplay.clone().spawn();
    // The version asked for, else the user's version.
    let explicit = format!("/api/playback/trickplay?kind=movie&id={id}&media_file_id={sd}");
    let mut info = Value::Null;
    for _ in 0..300 {
        info = f.get(&f.admin, &explicit).await;
        if info["status"] != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(info["status"], "ready", "{info}");
    let dir = f.cache.path().join("trickplay");
    assert!(dir.join(sd.to_string()).join("0.jpg").exists());
    assert!(
        !dir.join(hd.to_string()).exists(),
        "only the version played"
    );

    f.play(
        &f.admin,
        json!({ "kind": "movie", "id": id, "capabilities": caps(), "media_file_id": sd }),
    )
    .await;
    let mine = f
        .get(
            &f.admin,
            &format!("/api/playback/trickplay?kind=movie&id={id}"),
        )
        .await;
    assert_eq!(mine["status"], "ready", "the SD copy's: {mine}");
}

#[tokio::test]
async fn copies_of_an_episode_are_its_versions() {
    if !ffmpeg_available() {
        return;
    }
    let f = fixture().await;
    let season = "Shows/Dark (2017)/Season 01";
    make_video(
        f.media.path(),
        &format!("{season}/Dark - S01E01 720p.mp4"),
        "1280x720",
        2,
    );
    make_video(
        f.media.path(),
        &format!("{season}/Dark - S01E01 SD.mp4"),
        "320x240",
        2,
    );
    f.library("series", "Shows").await;
    let series = f.get(&f.admin, "/api/series").await["items"][0]["id"]
        .as_i64()
        .unwrap();
    let episodes = f
        .get(&f.admin, &format!("/api/series/{series}/seasons/1"))
        .await;
    assert_eq!(episodes.as_array().unwrap().len(), 1, "{episodes}");
    let id = episodes[0]["id"].as_i64().unwrap();
    let detail = f.get(&f.admin, &format!("/api/episodes/{id}")).await;
    let labels: Vec<&str> = detail["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["720p · H.264", "240p · H.264"]);
    let sd = detail["versions"][1]["id"].as_i64().unwrap();
    let (status, info) = f
        .play(
            &f.admin,
            json!({ "kind": "episode", "id": id, "capabilities": caps(), "media_file_id": sd }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["media_file_id"], sd);
    let detail = f.get(&f.admin, &format!("/api/episodes/{id}")).await;
    assert_eq!(detail["file"]["id"], sd);
}
