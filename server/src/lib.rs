pub mod auth;
pub mod config;
pub mod db;
pub mod downloads;
pub mod error;
pub mod ffmpeg;
pub mod library;
pub mod metadata;
pub mod playback;
pub mod routes;
pub mod userdata;

use std::sync::Arc;

use axum::{
    Json, Router,
    http::StatusCode,
    routing::{delete, get, post},
};
use tokio::sync::RwLock;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use crate::{
    auth::AuthState,
    config::Config,
    db::Db,
    downloads::encode::Encoder,
    ffmpeg::FfmpegInfo,
    library::scanner::ScanManager,
    metadata::tmdb::TmdbClient,
    playback::{signing::MediaSigner, transcode::TranscodeManager},
};

#[derive(OpenApi)]
#[openapi(
    info(title = "batvision", license(name = "AGPL-3.0-only")),
    modifiers(&BearerAuth),
    components(schemas(userdata::MarkableKind)),
    paths(
        routes::system::health,
        routes::system::info,
        routes::auth::setup_status,
        routes::auth::setup,
        routes::auth::login,
        routes::auth::login_totp,
        routes::auth::logout,
        routes::auth::accept_invitation,
        routes::auth::quick_connect_start,
        routes::auth::quick_connect_poll,
        routes::auth::quick_connect_approve,
        routes::me::get_me,
        routes::me::update_me,
        routes::me::change_password,
        routes::me::totp_setup,
        routes::me::totp_enable,
        routes::me::totp_disable,
        routes::me::regenerate_recovery_codes,
        routes::me::list_devices,
        routes::me::revoke_device,
        routes::admin::list_users,
        routes::admin::create_user,
        routes::admin::delete_user,
        routes::admin::create_invitation,
        routes::admin::list_invitations,
        routes::admin::delete_invitation,
        routes::library::list_libraries,
        routes::library::create_library,
        routes::library::delete_library,
        routes::library::scan_library,
        routes::library::list_scans,
        routes::library::search_metadata,
        routes::library::identify_movie,
        routes::library::identify_series,
        routes::library::libraries,
        routes::library::movies,
        routes::library::movie,
        routes::library::series_list,
        routes::library::series,
        routes::library::season_episodes,
        routes::library::episode,
        routes::library::image,
        routes::home::report_progress,
        routes::home::set_played,
        routes::home::home,
        routes::home::search,
        routes::home::get_preferences,
        routes::home::put_preferences,
        routes::playback::start,
        routes::playback::stop,
        routes::playback::stream,
        routes::playback::subtitle,
        routes::playback::hls,
        routes::downloads::plan,
        routes::downloads::request,
        routes::downloads::confirm,
        routes::downloads::file,
        routes::downloads::progress_batch,
        routes::downloads::overview,
        routes::downloads::put_settings,
        routes::downloads::get_watchlist,
        routes::downloads::add_to_watchlist,
        routes::downloads::remove_from_watchlist,
        routes::downloads::subscribe,
        routes::downloads::unsubscribe,
        routes::downloads::add_manual,
        routes::downloads::remove_manual,
        routes::downloads::encodes,
        routes::downloads::delete_encode,
    ),
    tags(
        (name = "system", description = "Server status"),
        (name = "auth", description = "Sign-in flows"),
        (name = "me", description = "The signed-in user's account"),
        (name = "admin", description = "Administration (admins with 2FA only)"),
        (name = "library", description = "Browsing movies and series"),
        (name = "playback", description = "Playback, progress and watched state"),
        (name = "downloads", description = "Offline downloads: expected sets, watchlist, subscriptions"),
    )
)]
pub struct ApiDoc;

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub auth: Arc<AuthState>,
    /// Filled in by the background ffmpeg probe.
    pub ffmpeg: Arc<RwLock<Option<FfmpegInfo>>>,
    /// `None` without a TMDB token: libraries still work, without metadata.
    pub tmdb: Option<Arc<TmdbClient>>,
    pub scans: Arc<ScanManager>,
    /// Signs media URLs. Random per process unless replaced by the persisted key.
    pub signer: MediaSigner,
    pub transcodes: Arc<TranscodeManager>,
    /// Encodes download versions on request.
    pub encoder: Arc<Encoder>,
}

impl AppState {
    pub fn new(config: Config, db: Db) -> Self {
        let tmdb = config.tmdb_token.as_deref().map(|token| {
            Arc::new(TmdbClient::new(
                &config.tmdb_api_url,
                &config.tmdb_image_url,
                token,
            ))
        });
        let mut key = [0u8; 32];
        rand::fill(&mut key);
        let transcodes = Arc::new(TranscodeManager::new(
            config.cache_dir.join("transcodes"),
            config.ffmpeg.clone(),
        ));
        let encoder = Arc::new(Encoder::new(
            db.clone(),
            config.cache_dir.join("variants"),
            config.media_dir.clone(),
            config.ffmpeg.clone(),
            config.encode_concurrency,
            (config.encode_cache_gb as i64) * 1_000_000_000,
        ));
        Self {
            encoder,
            tmdb,
            scans: Arc::new(ScanManager::default()),
            signer: MediaSigner::new(key),
            transcodes,
            config: Arc::new(config),
            db,
            auth: Arc::new(AuthState::default()),
            ffmpeg: Arc::new(RwLock::new(None)),
        }
    }

    /// While no account exists, creates the one-time token needed to create the first admin.
    pub async fn init_setup_token(&self) -> anyhow::Result<Option<String>> {
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.db)
            .await?;
        if users > 0 {
            return Ok(None);
        }
        let token = match &self.config.setup_token {
            Some(token) => token.trim().to_owned(),
            None => auth::crypto::random_code(b"ABCDEFGHJKMNPQRSTUVWXYZ23456789", 24),
        };
        *self.auth.setup_token.lock().unwrap() = Some(token.clone());
        Ok(Some(token))
    }
}

pub fn app(state: AppState) -> Router {
    use routes::{admin, auth, downloads, home, library, me, playback, system};

    let api = Router::new()
        .route("/health", get(system::health))
        .route("/system/info", get(system::info))
        .route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
        // Auth
        .route("/setup", get(auth::setup_status).post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/login/totp", post(auth::login_totp))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/invitations/accept", post(auth::accept_invitation))
        .route("/auth/quick-connect/start", post(auth::quick_connect_start))
        .route("/auth/quick-connect/poll", post(auth::quick_connect_poll))
        .route(
            "/auth/quick-connect/approve",
            post(auth::quick_connect_approve),
        )
        // Me
        .route("/me", get(me::get_me).patch(me::update_me))
        .route("/me/password", axum::routing::put(me::change_password))
        .route("/me/totp/setup", post(me::totp_setup))
        .route("/me/totp/enable", post(me::totp_enable))
        .route("/me/totp/disable", post(me::totp_disable))
        .route(
            "/me/totp/recovery-codes",
            post(me::regenerate_recovery_codes),
        )
        .route("/me/devices", get(me::list_devices))
        .route("/me/devices/{id}", delete(me::revoke_device))
        // Admin
        .route(
            "/admin/users",
            get(admin::list_users).post(admin::create_user),
        )
        .route("/admin/users/{id}", delete(admin::delete_user))
        .route(
            "/admin/invitations",
            get(admin::list_invitations).post(admin::create_invitation),
        )
        .route("/admin/invitations/{id}", delete(admin::delete_invitation))
        .route(
            "/admin/libraries",
            get(library::list_libraries).post(library::create_library),
        )
        .route("/admin/libraries/{id}", delete(library::delete_library))
        .route("/admin/libraries/{id}/scan", post(library::scan_library))
        .route("/admin/scans", get(library::list_scans))
        .route("/admin/metadata/search", get(library::search_metadata))
        .route("/admin/movies/{id}/identify", post(library::identify_movie))
        .route(
            "/admin/series/{id}/identify",
            post(library::identify_series),
        )
        // Library
        .route("/libraries", get(library::libraries))
        .route("/movies", get(library::movies))
        .route("/movies/{id}", get(library::movie))
        .route("/series", get(library::series_list))
        .route("/series/{id}", get(library::series))
        .route(
            "/series/{id}/seasons/{season}",
            get(library::season_episodes),
        )
        .route("/episodes/{id}", get(library::episode))
        .route("/images/{size}/{file}", get(library::image))
        .route("/home", get(home::home))
        .route("/search", get(home::search))
        .route("/progress", post(home::report_progress))
        .route("/played/{kind}/{id}", axum::routing::put(home::set_played))
        .route("/downloads/plan", post(downloads::plan))
        .route("/downloads/request", post(downloads::request))
        .route("/downloads/confirm", post(downloads::confirm))
        .route("/downloads/file/{token}", get(downloads::file))
        .route("/progress/batch", post(downloads::progress_batch))
        .route("/downloads/overview", get(downloads::overview))
        .route(
            "/downloads/devices/{id}/settings",
            axum::routing::put(downloads::put_settings),
        )
        .route("/watchlist", get(downloads::get_watchlist))
        .route(
            "/watchlist/{movie_id}",
            axum::routing::put(downloads::add_to_watchlist)
                .delete(downloads::remove_from_watchlist),
        )
        .route("/downloads/subscriptions", post(downloads::subscribe))
        .route(
            "/downloads/subscriptions/{id}",
            delete(downloads::unsubscribe),
        )
        .route("/downloads/manual", post(downloads::add_manual))
        .route("/downloads/manual/{id}", delete(downloads::remove_manual))
        .route("/admin/encodes", get(downloads::encodes))
        .route("/admin/encodes/{id}", delete(downloads::delete_encode))
        .route("/playback", post(playback::start))
        .route("/playback/stop", post(playback::stop))
        .route("/stream/{token}", get(playback::stream))
        .route("/subtitles/{token}", get(playback::subtitle))
        .route("/hls/{session}/{file}", get(playback::hls))
        .route(
            "/me/preferences",
            get(home::get_preferences).put(home::put_preferences),
        )
        // Without this, unknown /api paths would inherit the SPA fallback and return HTML.
        .fallback(|| async { StatusCode::NOT_FOUND });

    let mut router = Router::new().nest("/api", api);

    // Serve the web client as a SPA: unknown paths fall back to index.html
    // so client-side routes survive a page reload.
    if let Some(web_dir) = &state.config.web_dir {
        let index = web_dir.join("index.html");
        router = router.fallback_service(ServeDir::new(web_dir).fallback(ServeFile::new(index)));
    }

    router.layer(TraceLayer::new_for_http()).with_state(state)
}
