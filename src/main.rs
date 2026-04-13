mod commands;
mod config;
mod handlers;
mod matrix;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::sync::Arc;
use tera::Tera;
use tower_http::services::ServeDir;
use tower_sessions::{MemoryStore, SessionManagerLayer};
use tracing::info;

#[derive(Parser)]
#[command(
    name = "tuwunel-admin",
    about = "Web admin UI for tuwunel (Matrix chat server"
)]
struct Cli {
    /// Path to config.
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

pub struct Ctx {
    pub config: config::Config,
    pub tera: Tera,
    pub matrix: matrix::Matrix,
}


#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tuwunel_admin=info,tower_http=info".into()),
        )
        .init();

    // Init config.
    let cli = Cli::parse();
    let cfg = config::Config::load(&cli.config)?;
    let bind = cfg.server.bind.clone();

    // Init templates.
    let tera = Tera::new("templates/**/*.html")?;
    let matrix = matrix::Matrix::new(&cfg.matrix.homeserver);
    let state = Arc::new(Ctx {
        config: cfg,
        tera,
        matrix,
    });

    // Setup a simple in-memory sess store.
    let sess = SessionManagerLayer::new(MemoryStore::default())
        .with_secure(false)
        .with_same_site(tower_sessions::cookie::SameSite::Lax);

    let app = Router::new()
        .route("/", get(handlers::index))
        .route(
            "/login",
            get(handlers::login_page).post(handlers::login_submit),
        )
        .route("/logout", post(handlers::logout))
        .route("/m/:module", get(handlers::module_page))
        .route("/cmd/:module/:action", post(handlers::run_command))
        .nest_service("/static", ServeDir::new("static"))
        .layer(sess)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    info!("listening on {bind}");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
