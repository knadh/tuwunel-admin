use axum::{
    extract::{Form, Query, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, checkbox, cmd_flag, insert_flash, install_log, redirect_with_err, render,
    run_and_redirect, split_lines, take_flash, validate_line, with_fenced_payload,
};
use crate::{matrix, media, Ctx};

#[derive(Deserialize)]
pub struct LookupQuery {
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
    pub include_local: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteFromUserForm {
    pub user: String,
}

#[derive(Deserialize)]
pub struct DeleteFromServerForm {
    pub server: String,
    pub include_local: Option<String>,
}

#[derive(Deserialize)]
pub struct FetchRemoteForm {
    pub mxc: String,
    pub server: Option<String>,
    pub timeout: Option<String>,
    pub thumbnail: Option<String>,
    pub width: Option<String>,
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
        return redirect_with_err(&session, "MXC URL is required.", "/media").await;
    }
    if !validate_line(mxc) {
        return redirect_with_err(&session, "MXC URL cannot contain line breaks.", "/media").await;
    }
    let cmd = format!("media delete --mxc {mxc}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted {mxc}"),
        "/media",
    )
    .await
}

pub async fn delete_by_event(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteByEventForm>,
) -> Response {
    let evt = f.event_id.trim();
    if evt.is_empty() {
        return redirect_with_err(&session, "Event ID is required.", "/media").await;
    }
    if !validate_line(evt) {
        return redirect_with_err(&session, "Event ID cannot contain line breaks.", "/media").await;
    }
    let cmd = format!("media delete-by-event --event-id {evt}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted media for {evt}"),
        "/media",
    )
    .await
}

pub async fn delete_list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteListForm>,
) -> Response {
    let mxcs = split_lines(&f.mxcs);
    if mxcs.is_empty() {
        return redirect_with_err(&session, "Provide a list of MXC URLs.", "/media").await;
    }
    let n = mxcs.len();
    let cmd = with_fenced_payload("media delete-list", &mxcs.join("\n"));
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted {n} media file(s)"),
        "/media",
    )
    .await
}

pub async fn delete_range(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteRangeForm>,
) -> Response {
    let duration = f.duration.trim();
    if duration.is_empty() {
        return redirect_with_err(&session, "Duration is required.", "/media").await;
    }
    if !validate_line(duration) {
        return redirect_with_err(&session, "Duration cannot contain line breaks.", "/media").await;
    }
    let newer = f.direction == "newer";
    let mut cmd = String::from("media delete-range");
    cmd.push_str(if newer {
        " --newer-than"
    } else {
        " --older-than"
    });
    if checkbox(f.include_local.as_deref()) {
        cmd.push_str(" --yes-i-want-to-delete-local-media");
    }
    cmd.push(' ');
    cmd.push_str(duration);
    let msg = if newer {
        format!("Deleted media newer than {duration}")
    } else {
        format!("Deleted media older than {duration}")
    };
    run_and_redirect(&st, &sess, &session, &cmd, &msg, "/media").await
}

pub async fn delete_from_user(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteFromUserForm>,
) -> Response {
    let user = f.user.trim();
    if user.is_empty() {
        return redirect_with_err(&session, "User ID is required.", "/media").await;
    }
    if !validate_line(user) {
        return redirect_with_err(&session, "User ID cannot contain line breaks.", "/media").await;
    }
    let cmd = format!("media delete-all-from-user {user}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted all media from {user}"),
        "/media",
    )
    .await
}

pub async fn delete_from_server(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeleteFromServerForm>,
) -> Response {
    let server = f.server.trim();
    if server.is_empty() {
        return redirect_with_err(&session, "Server name is required.", "/media").await;
    }
    if !validate_line(server) {
        return redirect_with_err(&session, "Server name cannot contain line breaks.", "/media")
            .await;
    }
    let mut cmd = String::from("media delete-all-from-server");
    if checkbox(f.include_local.as_deref()) {
        cmd.push_str(" --yes-i-want-to-delete-local-media");
    }
    cmd.push(' ');
    cmd.push_str(server);
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted media from {server}"),
        "/media",
    )
    .await
}

pub async fn fetch_remote(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<FetchRemoteForm>,
) -> Response {
    let mxc = f.mxc.trim();
    if mxc.is_empty() {
        return redirect_with_err(&session, "MXC URL is required.", "/media").await;
    }
    if !validate_line(mxc) {
        return redirect_with_err(&session, "MXC URL cannot contain line breaks.", "/media").await;
    }
    let thumb = checkbox(f.thumbnail.as_deref());
    let mut cmd = String::from(if thumb {
        "media get-remote-thumbnail"
    } else {
        "media get-remote-file"
    });
    cmd_flag(&mut cmd, "server", f.server.as_ref());
    cmd_flag(&mut cmd, "timeout", f.timeout.as_ref());
    if thumb {
        cmd_flag(&mut cmd, "width", f.width.as_ref());
        cmd_flag(&mut cmd, "height", f.height.as_ref());
    }
    cmd.push(' ');
    cmd.push_str(mxc);
    let to = format!("/media?mxc={}", urlencoding::encode(mxc));
    run_and_redirect(&st, &sess, &session, &cmd, &format!("Fetched {mxc}"), &to).await
}
