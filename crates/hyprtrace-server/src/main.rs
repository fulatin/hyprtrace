mod ai;
mod config;
mod db;
mod models;
mod routes;

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    let cfg = config::Config::load()?;
    let db_path = cfg.db_path_expanded();
    let db = db::Database::open(&db_path)?;
    let ai = ai::AiManager::from_config(&cfg.ai);

    let state = Arc::new(routes::AppState {
        db: tokio::sync::Mutex::new(db),
        config: tokio::sync::Mutex::new(cfg.clone()),
        ai: tokio::sync::Mutex::new(ai),
    });

    let web_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        std::path::PathBuf::from(home).join(".local/share/hyprtrace/web")
    };

    let router = routes::create_router(state)
        .fallback_service(tower_http::services::ServeDir::new(&web_dir))
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    log::info!("HyprTrace API server starting at http://{}", addr);
    log::info!("Serving static files from {:?}", web_dir);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
