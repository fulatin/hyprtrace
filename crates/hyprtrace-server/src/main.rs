mod ai;
mod config;
mod db;
mod desktop;
mod models;
mod proactive;
mod retention;
mod routes;
mod weekly_report;

use anyhow::Context;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    let cfg = config::Config::load()?;
    let db_path = cfg.db_path_expanded();
    let db = db::Database::open(&db_path, cfg.daemon.focused_threshold_seconds)?;
    // The daemon may be running its DB migration (which writes the shared DB)
    // when the server starts. Retry startup writes with a backoff instead of
    // exiting — a single failed write here used to make systemd crash-loop the
    // server for the whole migration window ("database is locked").
    let mut init_attempts = 0u32;
    loop {
        match db.ensure_categories().and_then(|_| db.ensure_projects()) {
            Ok(()) => break,
            Err(e) if is_sqlite_busy(&e) && init_attempts < 5 => {
                init_attempts += 1;
                let delay = std::time::Duration::from_secs(2 * init_attempts as u64);
                log::warn!(
                    "Database startup init busy (attempt {init_attempts}/5), retrying in {}s: {:#}",
                    delay.as_secs(),
                    e
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e).context("Failed to initialize database"),
        }
    }
    let ai = ai::AiManager::from_config(&cfg.ai);

    let state = Arc::new(routes::AppState {
        db: tokio::sync::Mutex::new(db),
        config: tokio::sync::Mutex::new(cfg.clone()),
        ai: tokio::sync::Mutex::new(ai),
    });

    proactive::spawn_proactive_monitor(state.clone(), cfg.ai.proactive_interval_minutes);
    retention::spawn_retention_cleanup(state.clone());
    weekly_report::spawn_weekly_report_scheduler(state.clone());

    let web_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        std::path::PathBuf::from(home).join(".local/share/hyprtrace/web")
    };

    // SPA fallback: unknown non-API paths serve index.html (with a 200 status)
    // so client-side routes (/apps, /timeline, ...) survive a page refresh.
    // Note: use `.fallback()`, NOT `.not_found_service()` — the latter forces
    // a 404 status which breaks client-side routing.
    // API routes are nested under /api, so unmatched /api/* still returns a
    // proper 404 instead of HTML.
    let index_file = web_dir.join("index.html");
    let static_service = tower_http::services::ServeDir::new(&web_dir)
        .fallback(tower_http::services::ServeFile::new(&index_file));

    let router = axum::Router::new()
        .nest("/api", routes::create_router(state))
        .fallback_service(static_service)
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    log::info!("HyprTrace API server starting at http://{}", addr);
    log::info!("Serving static files from {:?}", web_dir);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

/// True when the error chain contains a SQLite busy/locked failure — transient
/// lock contention worth retrying with a backoff instead of crashing.
fn is_sqlite_busy(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(ffi, _))
                if ffi.code == rusqlite::ErrorCode::DatabaseBusy
                    || ffi.code == rusqlite::ErrorCode::DatabaseLocked
        )
    })
}
