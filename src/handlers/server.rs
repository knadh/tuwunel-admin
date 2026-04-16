use axum::{
    extract::{Form, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, checkbox, insert_flash, install_log, redirect_with_err, render, run_and_redirect,
    take_flash, validate_line,
};
use crate::{matrix, server, Ctx};

#[derive(Deserialize)]
pub struct ReloadConfigForm {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct AdminNoticeForm {
    pub message: String,
}

#[derive(Deserialize)]
pub struct RestartForm {
    pub force: Option<String>,
}

pub async fn index(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;
    let mut ctx = base_ctx(&st, &sess, "server");
    ctx.insert("tab", "admin");
    install_log(&mut ctx, flash.as_ref(), Vec::new());
    insert_flash(&mut ctx, flash);
    render(&st, "server/admin.html", &ctx)
}

pub async fn config_page(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;
    let mut ctx = base_ctx(&st, &sess, "server");
    ctx.insert("tab", "config");
    match server::config(&st.matrix, &sess).await {
        Ok(c) => {
            let log = c.log.clone();
            ctx.insert("config", &c);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "server/config.html", &ctx)
}

pub async fn stats_page(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;
    let mut ctx = base_ctx(&st, &sess, "server");
    ctx.insert("tab", "stats");
    match server::stats(&st.matrix, &sess).await {
        Ok(s) => {
            let log = s.log.clone();
            ctx.insert("stats", &s);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "server/stats.html", &ctx)
}

#[derive(Deserialize)]
pub struct RawCmdForm {
    pub cmd: String,
}

#[derive(Deserialize)]
pub struct ServerArgForm {
    pub server: String,
}

pub async fn raw_command(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<RawCmdForm>,
) -> Response {
    let cmd = f.cmd.trim();
    if cmd.is_empty() {
        return redirect_with_err(&session, "Command is required.", "/server").await;
    }
    run_and_redirect(&st, &sess, &session, cmd, "Ran admin command", "/server").await
}

pub async fn federation_ping(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<ServerArgForm>,
) -> Response {
    let server = f.server.trim();
    if server.is_empty() {
        return redirect_with_err(&session, "Server name is required.", "/server").await;
    }
    if !validate_line(server) {
        return redirect_with_err(&session, "Server name cannot contain line breaks.", "/server")
            .await;
    }
    let cmd = format!("debug ping {server}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Pinged {server}"),
        "/server",
    )
    .await
}

pub async fn federation_resolve(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<ServerArgForm>,
) -> Response {
    let server = f.server.trim();
    if server.is_empty() {
        return redirect_with_err(&session, "Server name is required.", "/server").await;
    }
    if !validate_line(server) {
        return redirect_with_err(&session, "Server name cannot contain line breaks.", "/server")
            .await;
    }
    let cmd = format!("debug resolve-true-destination {server}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Resolved {server}"),
        "/server",
    )
    .await
}

pub async fn reload_config(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<ReloadConfigForm>,
) -> Response {
    let path = f.path.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(p) = path {
        if !validate_line(p) {
            return redirect_with_err(&session, "Path cannot contain line breaks.", "/server")
                .await;
        }
    }
    let cmd = match path {
        Some(p) => format!("server reload-config {p}"),
        None => "server reload-config".to_string(),
    };
    run_and_redirect(&st, &sess, &session, &cmd, "Reloaded config", "/server").await
}

pub async fn clear_caches(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_redirect(
        &st,
        &sess,
        &session,
        "server clear-caches",
        "Cleared caches",
        "/server",
    )
    .await
}

pub async fn backup(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_redirect(
        &st,
        &sess,
        &session,
        "server backup-database",
        "Database backup triggered",
        "/server",
    )
    .await
}

pub async fn admin_notice(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<AdminNoticeForm>,
) -> Response {
    let msg = f.message.trim();
    if msg.is_empty() {
        return redirect_with_err(&session, "Message is required.", "/server").await;
    }
    let cmd = format!("server admin-notice {msg}");
    run_and_redirect(&st, &sess, &session, &cmd, "Sent admin notice", "/server").await
}

pub async fn reload_mods(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_redirect(
        &st,
        &sess,
        &session,
        "server reload-mods",
        "Reloaded server modules",
        "/server",
    )
    .await
}

pub async fn restart(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<RestartForm>,
) -> Response {
    let cmd = if checkbox(f.force.as_deref()) {
        "server restart --force"
    } else {
        "server restart"
    };
    run_and_redirect(&st, &sess, &session, cmd, "Restart requested", "/server").await
}

pub async fn shutdown(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    run_and_redirect(
        &st,
        &sess,
        &session,
        "server shutdown",
        "Shutdown requested",
        "/server",
    )
    .await
}
