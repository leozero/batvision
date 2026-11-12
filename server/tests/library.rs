mod common;

use std::{path::Path, time::Duration};

use axum::http::{Method, StatusCode};
use batvision_server::library::scanner;
use common::{TestApp, test_config};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

struct Fixture {
    app: TestApp,
    admin: String,
    media: tempfile::TempDir,
    _data: tempfile::TempDir,
    tmdb: Option<MockServer>,
}

fn touch(root: &Path, relative: &str) {
    let full = root.join(relative);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, b"not really a video").unwrap();
}

async fn fixture(with_tmdb: bool) -> Fixture {
    let media = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let mut config = test_config();
    config.media_dir = media.path().to_path_buf();
    config.data_dir = data.path().to_path_buf();
    config.ffprobe = "/nonexistent/ffprobe".into();
    let tmdb = if with_tmdb {
        let server = MockServer::start().await;
        mount_tmdb(&server).await;
        config.tmdb_token = Some("test-token".into());
        config.tmdb_api_url = server.uri();
        config.tmdb_image_url = server.uri();
        Some(server)
    } else {
        None
    };
    let app = TestApp::with_config(config).await;
    let (admin, _) = app.ready_admin().await;
    Fixture {
        app,
        admin,
        media,
        _data: data,
        tmdb,
    }
}

async fn mount_tmdb(server: &MockServer) {
    let json = |v: Value| ResponseTemplate::new(200).set_body_json(v);
    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .and(query_param("query", "Inception"))
        .respond_with(json(json!({ "results": [
            { "id": 27205, "title": "Inception", "release_date": "2010-07-15" }
        ]})))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .respond_with(json(json!({ "results": [] })))
        .mount(server)
        .await;
    Mock::given(path("/movie/27205"))
        .and(query_param("language", "fr-FR"))
        .respond_with(json(json!({
            "id": 27205, "title": "Inception", "overview": "Dom Cobb est un voleur.",
            "release_date": "2010-07-15", "runtime": 148,
            "poster_path": "/poster-fr.jpg", "backdrop_path": "/backdrop.jpg",
            "vote_average": 8.369, "vote_count": 37000
        })))
        .mount(server)
        .await;
    Mock::given(path("/movie/27205"))
        .and(query_param("language", "en-US"))
        .respond_with(json(json!({
            "id": 27205, "title": "Inception", "overview": "Dom Cobb is a thief.",
            "release_date": "2010-07-15", "runtime": 148,
            "poster_path": "/poster-en.jpg", "backdrop_path": "/backdrop.jpg",
            "vote_average": 8.369, "vote_count": 37000
        })))
        .mount(server)
        .await;
    Mock::given(path("/movie/155"))
        .respond_with(json(json!({
            "id": 155, "title": "The Dark Knight", "overview": "Batman.",
            "release_date": "2008-07-16", "poster_path": "/tdk.jpg",
            // Too few votes to be shown.
            "vote_average": 9.5, "vote_count": 3
        })))
        .mount(server)
        .await;
    Mock::given(path("/search/tv"))
        .and(query_param("query", "Dark"))
        .respond_with(json(json!({ "results": [
            { "id": 70523, "name": "Dark", "first_air_date": "2017-12-01" }
        ]})))
        .mount(server)
        .await;
    // No French overview: the English one must be used instead.
    Mock::given(path("/tv/70523"))
        .and(query_param("language", "fr-FR"))
        .respond_with(json(json!({
            "id": 70523, "name": "Dark", "overview": "", "first_air_date": "2017-12-01",
            "poster_path": "/dark.jpg", "seasons": [{ "season_number": 1 }],
            "vote_average": 8.4, "vote_count": 6500
        })))
        .mount(server)
        .await;
    Mock::given(path("/tv/70523"))
        .and(query_param("language", "en-US"))
        .respond_with(json(json!({
            "id": 70523, "name": "Dark", "overview": "A missing child.",
            "first_air_date": "2017-12-01", "poster_path": "/dark.jpg",
            "seasons": [{ "season_number": 1 }]
        })))
        .mount(server)
        .await;
    Mock::given(path("/tv/70523/season/1"))
        .and(query_param("language", "fr-FR"))
        .respond_with(json(json!({
            "season_number": 1, "name": "Saison 1", "overview": "", "vote_average": 8.2,
            "episodes": [
                { "episode_number": 1, "name": "Secrets", "overview": "Résumé 1", "still_path": "/e1.jpg",
                  "vote_average": 7.9, "vote_count": 30 },
                { "episode_number": 2, "name": "Mensonges", "overview": "Résumé 2",
                  "vote_average": 8.1, "vote_count": 20 },
                { "episode_number": 3, "name": "Passé et présent", "overview": "Résumé 3",
                  "vote_average": 9.0, "vote_count": 2 }
            ]
        })))
        .mount(server)
        .await;
    Mock::given(path("/tv/70523/season/1"))
        .and(query_param("language", "en-US"))
        .respond_with(json(json!({
            "season_number": 1, "name": "Season 1", "overview": "Season overview",
            "episodes": [
                { "episode_number": 1, "name": "Secrets", "overview": "Summary 1" },
                { "episode_number": 2, "name": "Lies", "overview": "Summary 2" },
                { "episode_number": 3, "name": "Past and Present", "overview": "Summary 3" }
            ]
        })))
        .mount(server)
        .await;
    Mock::given(path("/w500/poster-fr.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"JPEGDATA".to_vec()))
        .mount(server)
        .await;
}

impl Fixture {
    async fn create_library(&self, name: &str, kind: &str, path: &str) -> i64 {
        let (status, body) = self
            .app
            .post(
                "/api/admin/libraries",
                Some(&self.admin),
                json!({ "name": name, "kind": kind, "path": path }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id = body["id"].as_i64().unwrap();
        self.wait_for_scan(id).await;
        id
    }

    async fn wait_for_scan(&self, id: i64) {
        for _ in 0..500 {
            if !self.app.state.scans.is_running(id) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("scan did not finish");
    }

    async fn rescan(&self, id: i64) -> scanner::ScanReport {
        scanner::run_scan(&self.app.state, id).await.unwrap()
    }

    async fn get(&self, uri: &str) -> Value {
        let (status, body) = self.app.get(uri, Some(&self.admin)).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body
    }

    fn titles(page: &Value) -> Vec<String> {
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["title"].as_str().unwrap().to_owned())
            .collect()
    }
}

#[tokio::test]
async fn library_path_must_exist_inside_media_root() {
    let f = fixture(false).await;
    for bad in ["../outside", "Missing"] {
        let (status, err) = f
            .app
            .post(
                "/api/admin/libraries",
                Some(&f.admin),
                json!({ "name": "x", "kind": "movies", "path": bad }),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(err["code"], "invalid_path");
    }
}

#[tokio::test]
async fn library_management_is_admin_only() {
    let f = fixture(false).await;
    let alice = f.app.create_user(&f.admin, "alice").await;
    let (status, _) = f.app.get("/api/admin/libraries", Some(&alice)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = f.app.get("/api/movies", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn scans_movies_without_tmdb() {
    let f = fixture(false).await;
    touch(
        f.media.path(),
        "Films/Inception (2010)/Inception (2010).mkv",
    );
    touch(f.media.path(), "Films/Heat (1995).mp4");
    touch(
        f.media.path(),
        "Films/Inception (2010)/Extras/Making of.mkv",
    );
    let id = f.create_library("Films", "movies", "Films").await;

    let page = f.get("/api/movies").await;
    assert_eq!(page["total"], 2);
    assert_eq!(Fixture::titles(&page), vec!["Heat", "Inception"]);

    let status = &f.get("/api/admin/scans").await[0];
    assert_eq!(status["library_id"], id);
    assert_eq!(status["report"]["movies"], 2);
    assert_eq!(
        status["report"]["probe_errors"], 2,
        "fake files can't be probed"
    );
    assert!(status["report"]["metadata_skipped"].is_string());

    let movie_id = page["items"][1]["id"].as_i64().unwrap();
    let detail = f.get(&format!("/api/movies/{movie_id}")).await;
    assert_eq!(detail["year"], 2010);
    assert!(detail["file"]["size_bytes"].as_i64().unwrap() > 0);
    assert!(detail["file"]["media"].is_null());
}

#[tokio::test]
async fn missing_files_hide_items_and_returning_files_restore_them() {
    let f = fixture(false).await;
    touch(f.media.path(), "Films/Heat (1995)/Heat.mkv");
    touch(f.media.path(), "Films/Ronin (1998)/Ronin.mkv");
    let id = f.create_library("Films", "movies", "Films").await;
    let before = f.get("/api/movies").await;
    let heat_id = before["items"][0]["id"].clone();

    std::fs::remove_dir_all(f.media.path().join("Films/Heat (1995)")).unwrap();
    let report = f.rescan(id).await;
    assert_eq!(report.newly_missing, 1);
    assert_eq!(Fixture::titles(&f.get("/api/movies").await), vec!["Ronin"]);
    let (status, _) = f
        .app
        .get(&format!("/api/movies/{heat_id}"), Some(&f.admin))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    touch(f.media.path(), "Films/Heat (1995)/Heat.mkv");
    f.rescan(id).await;
    let after = f.get("/api/movies").await;
    assert_eq!(
        after["items"][0]["id"], heat_id,
        "same id: user data stays attached"
    );
}

#[tokio::test]
async fn fetches_movie_metadata_in_each_users_language() {
    let f = fixture(true).await;
    let alice = f.app.create_user(&f.admin, "alice").await;
    f.app
        .request(
            Method::PATCH,
            "/api/me",
            Some(&alice),
            Some(json!({ "language": "en-US" })),
        )
        .await;
    touch(f.media.path(), "Films/Inception (2010)/Inception.mkv");
    f.create_library("Films", "movies", "Films").await;

    let page = f.get("/api/movies").await;
    let id = page["items"][0]["id"].as_i64().unwrap();
    assert_eq!(page["items"][0]["poster_path"], "/poster-fr.jpg");

    let fr = f.get(&format!("/api/movies/{id}")).await;
    assert_eq!(fr["overview"], "Dom Cobb est un voleur.");
    assert_eq!(fr["tmdb_id"], 27205);
    assert_eq!(fr["runtime_minutes"], 148);

    let (_, en) = f.app.get(&format!("/api/movies/{id}"), Some(&alice)).await;
    assert_eq!(en["overview"], "Dom Cobb is a thief.");
}

#[tokio::test]
async fn unmatched_movies_keep_their_parsed_title() {
    let f = fixture(true).await;
    touch(f.media.path(), "Films/Film Inconnu (2003)/film.mkv");
    let id = f.create_library("Films", "movies", "Films").await;
    assert_eq!(
        Fixture::titles(&f.get("/api/movies").await),
        vec!["Film Inconnu"]
    );
    // Not searched again on the next scan (retried only after a week).
    let report = f.rescan(id).await;
    assert_eq!(report.metadata_refreshed, 0);
}

#[tokio::test]
async fn unmatched_movies_are_searched_again_when_their_parsed_title_changes() {
    let f = fixture(true).await;
    touch(f.media.path(), "Films/Inception (2010)/inception.mkv");
    let id = f.create_library("Films", "movies", "Films").await;
    // As left by an older parser: a garbled title that TMDB could not match.
    sqlx::query(
        "UPDATE movies SET parsed_title = 'Inception 1080p', tmdb_id = NULL,
         metadata_updated_at = unixepoch()",
    )
    .execute(&f.app.state.db)
    .await
    .unwrap();

    let report = f.rescan(id).await;
    assert_eq!(report.metadata_refreshed, 1);
    let page = f.get("/api/movies").await;
    let movie = page["items"][0]["id"].as_i64().unwrap();
    let detail = f.get(&format!("/api/movies/{movie}")).await;
    assert_eq!(detail["tmdb_id"], 27205);
}

#[tokio::test]
async fn manual_identification_sticks_across_scans() {
    let f = fixture(true).await;
    touch(f.media.path(), "Films/Inception (2010)/Inception.mkv");
    let library = f.create_library("Films", "movies", "Films").await;
    let id = f.get("/api/movies").await["items"][0]["id"]
        .as_i64()
        .unwrap();

    let (status, body) = f
        .app
        .post(
            &format!("/api/admin/movies/{id}/identify"),
            Some(&f.admin),
            json!({ "tmdb_id": 155 }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert_eq!(
        f.get(&format!("/api/movies/{id}")).await["title"],
        "The Dark Knight"
    );

    f.rescan(library).await;
    let detail = f.get(&format!("/api/movies/{id}")).await;
    assert_eq!(detail["tmdb_id"], 155);
    assert_eq!(detail["title"], "The Dark Knight");
}

#[tokio::test]
async fn admin_can_search_tmdb_candidates() {
    let f = fixture(true).await;
    let results = f
        .get("/api/admin/metadata/search?kind=movies&query=Inception")
        .await;
    assert_eq!(results[0]["tmdb_id"], 27205);
    assert_eq!(results[0]["year"], 2010);
}

#[tokio::test]
async fn scans_series_seasons_and_episodes_with_metadata() {
    let f = fixture(true).await;
    touch(
        f.media.path(),
        "Séries/Dark (2017)/Season 01/Dark - S01E01.mkv",
    );
    touch(
        f.media.path(),
        "Séries/Dark (2017)/Season 01/Dark - S01E02.mkv",
    );
    let library = f.create_library("Séries", "series", "Séries").await;

    let page = f.get("/api/series").await;
    assert_eq!(page["items"][0]["title"], "Dark");
    assert_eq!(page["items"][0]["episode_count"], 2);
    let id = page["items"][0]["id"].as_i64().unwrap();

    let detail = f.get(&format!("/api/series/{id}")).await;
    assert_eq!(
        detail["overview"], "A missing child.",
        "falls back to English"
    );
    assert_eq!(detail["seasons"][0]["season_number"], 1);
    assert_eq!(detail["seasons"][0]["title"], "Saison 1");

    let episodes = f.get(&format!("/api/series/{id}/seasons/1")).await;
    assert_eq!(episodes[1]["title"], "Mensonges");
    assert_eq!(episodes[0]["still_path"], "/e1.jpg");

    // A new episode gets its metadata on the next scan.
    touch(
        f.media.path(),
        "Séries/Dark (2017)/Season 01/Dark - S01E03.mkv",
    );
    let report = f.rescan(library).await;
    assert_eq!(report.metadata_refreshed, 1);
    let episodes = f.get(&format!("/api/series/{id}/seasons/1")).await;
    assert_eq!(episodes[2]["title"], "Passé et présent");

    let episode_id = episodes[2]["id"].as_i64().unwrap();
    let episode = f.get(&format!("/api/episodes/{episode_id}")).await;
    assert_eq!(episode["series_title"], "Dark");
    assert_eq!(episode["episode_number"], 3);
    assert!(episode["file"].is_object());
}

#[tokio::test]
async fn shows_tmdb_ratings_with_enough_votes() {
    let f = fixture(true).await;
    touch(f.media.path(), "Films/Inception (2010)/Inception.mkv");
    touch(f.media.path(), "Films/Film Inconnu (2003)/film.mkv");
    f.create_library("Films", "movies", "Films").await;

    // Unrated items go last, whatever their title.
    let page = f.get("/api/movies?sort=rating").await;
    assert_eq!(Fixture::titles(&page), vec!["Inception", "Film Inconnu"]);
    let rating = json!([{ "source": "tmdb", "value": 8.369, "scale": 10, "votes": 37000 }]);
    assert_eq!(page["items"][0]["ratings"], rating);
    assert_eq!(page["items"][1]["ratings"], json!([]));
    let id = page["items"][0]["id"].as_i64().unwrap();
    assert_eq!(f.get(&format!("/api/movies/{id}")).await["ratings"], rating);
    let home = f.get("/api/home").await;
    let recent = home["recent_movies"].as_array().unwrap();
    let inception = recent.iter().find(|m| m["id"] == id).unwrap();
    assert_eq!(inception["ratings"], rating);

    // The Dark Knight has only 3 votes: hidden.
    let (status, _) = f
        .app
        .post(
            &format!("/api/admin/movies/{id}/identify"),
            Some(&f.admin),
            json!({ "tmdb_id": 155 }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        f.get(&format!("/api/movies/{id}")).await["ratings"],
        json!([])
    );
}

#[tokio::test]
async fn rates_series_seasons_and_episodes() {
    let f = fixture(true).await;
    for n in 1..=3 {
        touch(
            f.media.path(),
            &format!("Séries/Dark (2017)/Season 01/Dark - S01E0{n}.mkv"),
        );
    }
    f.create_library("Séries", "series", "Séries").await;
    let page = f.get("/api/series?sort=rating").await;
    assert_eq!(page["items"][0]["ratings"][0]["value"], 8.4);
    let id = page["items"][0]["id"].as_i64().unwrap();

    let detail = f.get(&format!("/api/series/{id}")).await;
    assert_eq!(detail["ratings"][0]["votes"], 6500);
    // TMDB has no vote count for a season: its episodes' votes are used.
    assert_eq!(
        detail["seasons"][0]["ratings"],
        json!([{ "source": "tmdb", "value": 8.2, "scale": 10, "votes": 52 }])
    );

    let episodes = f.get(&format!("/api/series/{id}/seasons/1")).await;
    assert_eq!(episodes[0]["ratings"][0]["value"], 7.9);
    assert_eq!(episodes[2]["ratings"], json!([]), "only 2 votes");
    let episode_id = episodes[1]["id"].as_i64().unwrap();
    assert_eq!(
        f.get(&format!("/api/episodes/{episode_id}")).await["ratings"][0]["votes"],
        20
    );
}

#[tokio::test]
async fn stale_metadata_is_fetched_again() {
    let f = fixture(true).await;
    touch(f.media.path(), "Films/Inception (2010)/Inception.mkv");
    let library = f.create_library("Films", "movies", "Films").await;
    assert_eq!(f.rescan(library).await.metadata_refreshed, 0, "fresh");

    // As left by the ratings migration: matched, but without a rating.
    sqlx::query("DELETE FROM ratings")
        .execute(&f.app.state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE movies SET metadata_updated_at = 0")
        .execute(&f.app.state.db)
        .await
        .unwrap();
    assert_eq!(f.rescan(library).await.metadata_refreshed, 1);
    assert_eq!(
        f.get("/api/movies").await["items"][0]["ratings"][0]["votes"],
        37000
    );
    assert_eq!(f.rescan(library).await.metadata_refreshed, 0);
}

#[tokio::test]
async fn sorting_by_recently_added() {
    let f = fixture(false).await;
    touch(f.media.path(), "Films/Alpha (2000)/a.mkv");
    let library = f.create_library("Films", "movies", "Films").await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    touch(f.media.path(), "Films/Zulu (2000)/z.mkv");
    f.rescan(library).await;
    assert_eq!(
        Fixture::titles(&f.get("/api/movies?sort=added").await),
        vec!["Zulu", "Alpha"]
    );
    assert_eq!(
        Fixture::titles(&f.get("/api/movies?sort=title&limit=1").await),
        vec!["Alpha"]
    );
}

#[tokio::test]
async fn images_are_proxied_cached_and_restricted_to_the_library() {
    let f = fixture(true).await;
    touch(f.media.path(), "Films/Inception (2010)/Inception.mkv");
    f.create_library("Films", "movies", "Films").await;

    let (status, body) = f.app.get("/api/images/w500/poster-fr.jpg", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Value::String("JPEGDATA".into()));
    // Second time from the disk cache: TMDB is not called again.
    f.tmdb.as_ref().unwrap().reset().await;
    let (status, _) = f.app.get("/api/images/w500/poster-fr.jpg", None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = f.app.get("/api/images/w500/not-in-library.jpg", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = f.app.get("/api/images/w9999/poster-fr.jpg", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_library_removes_its_items() {
    let f = fixture(false).await;
    touch(f.media.path(), "Films/Heat (1995)/Heat.mkv");
    let id = f.create_library("Films", "movies", "Films").await;
    assert_eq!(f.get("/api/movies").await["total"], 1);
    let (status, _) = f
        .app
        .request(
            Method::DELETE,
            &format!("/api/admin/libraries/{id}"),
            Some(&f.admin),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(f.get("/api/movies").await["total"], 0);
    assert!(
        f.media.path().join("Films/Heat (1995)/Heat.mkv").exists(),
        "files untouched"
    );
}

/// Real ffprobe on a real (tiny) video. Skipped where ffmpeg isn't installed.
#[tokio::test]
async fn probes_real_files_with_ffprobe() {
    let ffmpeg_ok = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok()
        && std::process::Command::new("ffprobe")
            .arg("-version")
            .output()
            .is_ok();
    if !ffmpeg_ok {
        eprintln!("ffmpeg/ffprobe not installed: skipping");
        return;
    }
    let media = tempfile::tempdir().unwrap();
    let file = media.path().join("Films/Test (2020)/Test.mkv");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=2:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=2",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-metadata:s:a:0",
            "language=fre",
            "-shortest",
        ])
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());

    let mut config = test_config();
    config.media_dir = media.path().to_path_buf();
    let app = TestApp::with_config(config).await;
    let (admin, _) = app.ready_admin().await;
    let (_, library) = app
        .post(
            "/api/admin/libraries",
            Some(&admin),
            json!({ "name": "Films", "kind": "movies", "path": "Films" }),
        )
        .await;
    let library_id = library["id"].as_i64().unwrap();
    for _ in 0..500 {
        if !app.state.scans.is_running(library_id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let (_, page) = app.get("/api/movies", Some(&admin)).await;
    let id = page["items"][0]["id"].as_i64().unwrap();
    let (_, detail) = app.get(&format!("/api/movies/{id}"), Some(&admin)).await;
    let media = &detail["file"]["media"];
    assert_eq!(media["video"]["codec"], "h264");
    assert_eq!(media["video"]["width"], 320);
    assert_eq!(media["audio"][0]["language"], "fre");
    let duration = media["duration_ms"].as_i64().unwrap();
    assert!((1900..=2200).contains(&duration), "{duration}");
}
