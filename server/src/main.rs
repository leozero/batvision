use std::net::SocketAddr;

use batvision_server::{AppState, app, config::Config, db, ffmpeg};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        // Logs on stderr so `openapi` output on stdout stays pure JSON.
        .with_writer(std::io::stderr)
        .init();

    // `batvision-server openapi` prints the spec, used to generate the TS client.
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        use utoipa::OpenApi;
        println!("{}", batvision_server::ApiDoc::openapi().to_pretty_json()?);
        return Ok(());
    }

    let config = Config::from_env()?;
    for dir in [&config.data_dir, &config.cache_dir] {
        std::fs::create_dir_all(dir)?;
    }
    tracing::info!(?config, "starting batvision");

    let db = db::open(&config.data_dir).await?;
    let mut state = AppState::new(config.clone(), db);
    // Persisted so signed media URLs survive a restart.
    state.signer =
        batvision_server::playback::signing::MediaSigner::load_or_create(&config.data_dir)?;
    state.transcodes.clean();
    state.transcodes.clone().spawn_reaper();
    let probe = state.ffmpeg.clone();
    state
        .encoder
        .clone()
        .start(move || {
            probe
                .try_read()
                .ok()
                .and_then(|p| {
                    p.as_ref().map(|i| {
                        batvision_server::downloads::encode::Encoders::from_names(
                            &i.working_encoders,
                        )
                    })
                })
                .unwrap_or_default()
        })
        .await?;
    if let Some(token) = state.init_setup_token().await? {
        tracing::warn!(
            "no account exists yet: create the first administrator with setup token {token}"
        );
    }

    let probe_state = state.clone();
    tokio::spawn(async move {
        let info = ffmpeg::probe(&probe_state.config.ffmpeg).await;
        tracing::info!(version = ?info.version, encoders = ?info.working_encoders, "ffmpeg probe done");
        *probe_state.ffmpeg.write().await = Some(info);
    });

    if state.tmdb.is_none() {
        tracing::warn!("BATVISION_TMDB_TOKEN is not set: libraries will have no metadata");
    }
    // First tick runs right away: libraries are scanned at startup.
    batvision_server::library::scanner::spawn_scheduler(state.clone());

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!("listening on http://{}", config.bind);
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
