use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{base_ctx, insert_flash, install_log, render, run_and_flash, set_flash, take_flash};
use crate::{matrix, server, Ctx};

#[derive(Deserialize)]
pub struct ReloadConfigForm {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct AdminNoticeForm {
    pub message: String,
}

#[derive(Deserialize)]
pub struct RestartForm {
    #[serde(default)]
    pub force: Option<String>,
}

pub async fn index(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;
    let mut ctx = base_ctx(&st, &sess, "server");
    match server::overview(&st.matrix, &sess).await {
        Ok(o) => {
            let log = o.log.clone();
            ctx.insert("overview", &o);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "server/index.html", &ctx)
}

pub async fn reload_config(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<ReloadConfigForm>,
) -> Response {
    let cmd = match f.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => format!("server reload-config {p}"),
        None => "server reload-config".to_string(),
    };
    run_and_flash(&st, &sess, &session, &cmd, "Reloaded config").await;
    Redirect::to("/server").into_response()
}

pub async fn clear_caches(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_flash(
        &st,
        &sess,
        &session,
        "server clear-caches",
        "Cleared caches",
    )
    .await;
    Redirect::to("/server").into_response()
}

pub async fn backup(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_flash(
        &st,
        &sess,
        &session,
        "server backup-database",
        "Database backup triggered",
    )
    .await;
    Redirect::to("/server").into_response()
}

pub async fn admin_notice(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<AdminNoticeForm>,
) -> Response {
    let msg = f.message.trim();
    if msg.is_empty() {
        set_flash(&session, "error", "Message is required.").await;
        return Redirect::to("/server").into_response();
    }
    let cmd = format!("server admin-notice {msg}");
    run_and_flash(&st, &sess, &session, &cmd, "Sent admin notice").await;
    Redirect::to("/server").into_response()
}

pub async fn reload_mods(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_flash(
        &st,
        &sess,
        &session,
        "server reload-mods",
        "Reloaded server modules",
    )
    .await;
    Redirect::to("/server").into_response()
}

pub async fn restart(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<RestartForm>,
) -> Response {
    let cmd = if matches!(f.force.as_deref(), Some("on" | "true" | "1" | "yes")) {
        "server restart --force"
    } else {
        "server restart"
    };
    run_and_flash(&st, &sess, &session, cmd, "Restart requested").await;
    Redirect::to("/server").into_response()
}

pub async fn shutdown(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_flash(
        &st,
        &sess,
        &session,
        "server shutdown",
        "Shutdown requested",
    )
    .await;
    Redirect::to("/server").into_response()
}
