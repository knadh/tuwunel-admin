use axum::{
    extract::{Form, Query, State},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{base_ctx, insert_flash, install_log, render, run_and_flash, set_flash, take_flash};
use crate::{matrix, media, Ctx};

#[derive(Deserialize)]
pub struct LookupQuery {
    #[serde(default)]
    pub mxc: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteMxcForm {
    pub mxc: String,
}

#[derive(Deserialize)]
pub struct DeleteByEventForm {
    pub event_id: String,
}

#[derive(Deserialize)]
pub struct DeleteListForm {
    pub mxcs: String,
}

#[derive(Deserialize)]
pub struct DeleteRangeForm {
    pub duration: String,
    pub direction: String,
    #[serde(default)]
    pub include_local: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteFromUserForm {
    pub user: String,
}

#[derive(Deserialize)]
pub struct DeleteFromServerForm {
    pub server: String,
    #[serde(default)]
    pub include_local: Option<String>,
}

#[derive(Deserialize)]
pub struct FetchRemoteForm {
    pub mxc: String,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub width: Option<String>,
    #[serde(default)]
    pub height: Option<String>,
}

pub async fn index(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Query(q): Query<LookupQuery>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "media");
    let mxc = q.mxc.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(mxc) = mxc {
        ctx.insert("lookup_mxc", mxc);
        match media::file_info(&st.matrix, &sess, mxc).await {
            Ok((info, log)) => {
                ctx.insert("file_info", &info);
                install_log(&mut ctx, flash.as_ref(), log);
            }
            Err(e) => ctx.insert("error", &format!("{e:#}")),
        }
    }
    insert_flash(&mut ctx, flash);
    render(&st, "media/index.html", &ctx)
}

pub async fn delete(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteMxcForm>,
) -> Response {
    let mxc = f.mxc.trim();
    if mxc.is_empty() {
        set_flash(&session, "error", "MXC URL is required.").await;
        return Redirect::to("/media").into_response();
    }
    let cmd = format!("media delete --mxc {mxc}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Deleted {mxc}")).await;
    Redirect::to("/media").into_response()
}

pub async fn delete_by_event(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteByEventForm>,
) -> Response {
    let evt = f.event_id.trim();
    if evt.is_empty() {
        set_flash(&session, "error", "Event ID is required.").await;
        return Redirect::to("/media").into_response();
    }
    let cmd = format!("media delete-by-event --event-id {evt}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted media for {evt}"),
    )
    .await;
    Redirect::to("/media").into_response()
}

pub async fn delete_list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteListForm>,
) -> Response {
    let mxcs: Vec<&str> = f
        .mxcs
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if mxcs.is_empty() {
        set_flash(&session, "error", "Provide a list of MXC URLs.").await;
        return Redirect::to("/media").into_response();
    }
    let mut cmd = String::from("media delete-list\n```\n");
    cmd.push_str(&mxcs.join("\n"));
    cmd.push_str("\n```");
    let n = mxcs.len();
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted {n} media file(s)"),
    )
    .await;
    Redirect::to("/media").into_response()
}

pub async fn delete_range(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteRangeForm>,
) -> Response {
    let duration = f.duration.trim();
    if duration.is_empty() {
        set_flash(&session, "error", "Duration is required.").await;
        return Redirect::to("/media").into_response();
    }
    let mut cmd = String::from("media delete-range");
    match f.direction.as_str() {
        "newer" => cmd.push_str(" --newer-than"),
        _ => cmd.push_str(" --older-than"),
    }
    if matches!(
        f.include_local.as_deref(),
        Some("on" | "true" | "1" | "yes")
    ) {
        cmd.push_str(" --yes-i-want-to-delete-local-media");
    }
    cmd.push(' ');
    cmd.push_str(duration);
    let msg = match f.direction.as_str() {
        "newer" => format!("Deleted media newer than {duration}"),
        _ => format!("Deleted media older than {duration}"),
    };
    run_and_flash(&st, &sess, &session, &cmd, &msg).await;
    Redirect::to("/media").into_response()
}

pub async fn delete_from_user(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteFromUserForm>,
) -> Response {
    let user = f.user.trim();
    if user.is_empty() {
        set_flash(&session, "error", "User ID is required.").await;
        return Redirect::to("/media").into_response();
    }
    let cmd = format!("media delete-all-from-user {user}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted all media from {user}"),
    )
    .await;
    Redirect::to("/media").into_response()
}

pub async fn delete_from_server(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteFromServerForm>,
) -> Response {
    let server = f.server.trim();
    if server.is_empty() {
        set_flash(&session, "error", "Server name is required.").await;
        return Redirect::to("/media").into_response();
    }
    let mut cmd = String::from("media delete-all-from-server");
    if matches!(
        f.include_local.as_deref(),
        Some("on" | "true" | "1" | "yes")
    ) {
        cmd.push_str(" --yes-i-want-to-delete-local-media");
    }
    cmd.push(' ');
    cmd.push_str(server);
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted media from {server}"),
    )
    .await;
    Redirect::to("/media").into_response()
}

pub async fn fetch_remote(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<FetchRemoteForm>,
) -> Response {
    let mxc = f.mxc.trim();
    if mxc.is_empty() {
        set_flash(&session, "error", "MXC URL is required.").await;
        return Redirect::to("/media").into_response();
    }
    let thumb = matches!(f.thumbnail.as_deref(), Some("on" | "true" | "1" | "yes"));
    let mut cmd = String::from(if thumb {
        "media get-remote-thumbnail"
    } else {
        "media get-remote-file"
    });
    if let Some(server) = f.server.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        cmd.push_str(&format!(" --server {server}"));
    }
    if let Some(timeout) = f
        .timeout
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        cmd.push_str(&format!(" --timeout {timeout}"));
    }
    if thumb {
        if let Some(w) = f.width.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            cmd.push_str(&format!(" --width {w}"));
        }
        if let Some(h) = f.height.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            cmd.push_str(&format!(" --height {h}"));
        }
    }
    cmd.push(' ');
    cmd.push_str(mxc);
    run_and_flash(&st, &sess, &session, &cmd, &format!("Fetched {mxc}")).await;
    Redirect::to(&format!("/media?mxc={}", urlencoding::encode(mxc))).into_response()
}
