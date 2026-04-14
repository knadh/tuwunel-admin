mod appservices;
mod commands;
mod config;
mod federation;
mod handlers;
mod matrix;
mod media;
mod parse;
mod rooms;
mod server;
mod tokens;
mod users;

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

    let protected = Router::new()
        .route("/", get(handlers::index))
        // Users
        .route("/users", get(handlers::users::list))
        .route("/users/create", post(handlers::users::create))
        .route(
            "/users/deactivate-all",
            post(handlers::users::deactivate_all),
        )
        .route("/users/:mxid", get(handlers::users::detail))
        .route(
            "/users/:mxid/reset-password",
            post(handlers::users::reset_password),
        )
        .route("/users/:mxid/deactivate", post(handlers::users::deactivate))
        .route("/users/:mxid/make-admin", post(handlers::users::make_admin))
        .route("/users/:mxid/force-join", post(handlers::users::force_join))
        .route(
            "/users/:mxid/force-leave",
            post(handlers::users::force_leave),
        )
        .route(
            "/users/:mxid/redact-event",
            post(handlers::users::redact_event),
        )
        .route(
            "/users/:mxid/devices/:device_id/delete",
            post(handlers::users::delete_device),
        )
        .route(
            "/users/:mxid/rooms/:room_id/promote",
            post(handlers::users::force_promote),
        )
        .route(
            "/users/:mxid/rooms/:room_id/demote",
            post(handlers::users::force_demote),
        )
        .route(
            "/users/:mxid/rooms/:room_id/tag",
            post(handlers::users::room_tag),
        )
        .route(
            "/users/:mxid/rooms/:room_id/tags",
            get(handlers::users::get_room_tags),
        )
        // Rooms
        .route("/rooms", get(handlers::rooms::list))
        .route("/rooms/find-by-alias", get(handlers::rooms::find_by_alias))
        .route("/rooms/prune-empty", post(handlers::rooms::prune_empty))
        .route("/rooms/ban-list", post(handlers::rooms::ban_list))
        .route("/rooms/:room_id", get(handlers::rooms::detail))
        .route("/rooms/:room_id/ban", post(handlers::rooms::ban))
        .route("/rooms/:room_id/unban", post(handlers::rooms::unban))
        .route("/rooms/:room_id/delete", post(handlers::rooms::delete))
        .route(
            "/rooms/:room_id/federation/enable",
            post(handlers::rooms::federation_enable),
        )
        .route(
            "/rooms/:room_id/federation/disable",
            post(handlers::rooms::federation_disable),
        )
        .route(
            "/rooms/:room_id/directory/publish",
            post(handlers::rooms::directory_publish),
        )
        .route(
            "/rooms/:room_id/directory/unpublish",
            post(handlers::rooms::directory_unpublish),
        )
        .route("/rooms/:room_id/aliases", post(handlers::rooms::alias_add))
        .route(
            "/rooms/:room_id/aliases/remove",
            post(handlers::rooms::alias_remove),
        )
        .route(
            "/rooms/:room_id/force-join-users",
            post(handlers::rooms::force_join_users),
        )
        // Media
        .route("/media", get(handlers::media::index))
        .route("/media/delete", post(handlers::media::delete))
        .route(
            "/media/delete-by-event",
            post(handlers::media::delete_by_event),
        )
        .route("/media/delete-list", post(handlers::media::delete_list))
        .route("/media/delete-range", post(handlers::media::delete_range))
        .route(
            "/media/delete-from-user",
            post(handlers::media::delete_from_user),
        )
        .route(
            "/media/delete-from-server",
            post(handlers::media::delete_from_server),
        )
        .route("/media/fetch-remote", post(handlers::media::fetch_remote))
        // Tokens
        .route("/tokens", get(handlers::tokens::list))
        .route("/tokens/issue", post(handlers::tokens::issue))
        .route("/tokens/:token/revoke", post(handlers::tokens::revoke))
        // Appservices
        .route("/appservices", get(handlers::appservices::list))
        .route(
            "/appservices/register",
            post(handlers::appservices::register),
        )
        .route("/appservices/:id", get(handlers::appservices::detail))
        .route(
            "/appservices/:id/unregister",
            post(handlers::appservices::unregister),
        )
        // Federation
        .route("/federation", get(handlers::federation::index))
        .route(
            "/federation/fetch-well-known",
            post(handlers::federation::fetch_well_known),
        )
        .route(
            "/federation/remote-user-in-rooms",
            post(handlers::federation::remote_user_in_rooms),
        )
        // Server
        .route("/server", get(handlers::server::index))
        .route(
            "/server/reload-config",
            post(handlers::server::reload_config),
        )
        .route("/server/clear-caches", post(handlers::server::clear_caches))
        .route("/server/backup", post(handlers::server::backup))
        .route("/server/admin-notice", post(handlers::server::admin_notice))
        .route("/server/reload-mods", post(handlers::server::reload_mods))
        .route("/server/restart", post(handlers::server::restart))
        .route("/server/shutdown", post(handlers::server::shutdown))
        // Catch-all for diagnostics / anything else still in the module catalog
        .route("/m/:module", get(handlers::module_page))
        .route("/cmd/:module/:action", post(handlers::run_command))
        .route_layer(axum::middleware::from_fn(handlers::require_auth));

    let app = Router::new()
        .merge(protected)
        .route(
            "/login",
            get(handlers::login_page).post(handlers::login_submit),
        )
        .route("/logout", post(handlers::logout))
        .nest_service("/static", ServeDir::new("static"))
        .layer(sess)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    info!("listening on {bind}");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
