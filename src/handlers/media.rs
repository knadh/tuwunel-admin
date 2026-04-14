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
pub struct DeleteMediaForm {
    #[serde(default)]
    pub mxc: Option<String>,
    #[serde(default)]
    pub event_id: Option<String>,
}

#[derive(Deserialize)]
pub struct DeletePastForm {
    pub duration: String,
    #[serde(default)]
    pub after: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteFromUserForm {
    pub user: String,
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
    Form(f): Form<DeleteMediaForm>,
) -> Response {
    let mxc = f.mxc.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let event = f
        .event_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if mxc.is_none() && event.is_none() {
        set_flash(&session, "error", "Provide an MXC URL or event ID.").await;
        return Redirect::to("/media").into_response();
    }
    let mut cmd = String::from("media delete");
    if let Some(v) = mxc {
        cmd.push_str(&format!(" --mxc {v}"));
    }
    if let Some(v) = event {
        cmd.push_str(&format!(" --event-id {v}"));
    }
    let success = if let Some(v) = mxc {
        format!("Deleted {v}")
    } else {
        format!("Deleted media for {}", event.unwrap())
    };
    run_and_flash(&st, &sess, &session, &cmd, &success).await;
    Redirect::to("/media").into_response()
}

pub async fn delete_past(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeletePastForm>,
) -> Response {
    let duration = f.duration.trim();
    if duration.is_empty() {
        set_flash(&session, "error", "Duration is required.").await;
        return Redirect::to("/media").into_response();
    }
    let after = matches!(f.after.as_deref(), Some("on" | "true" | "1" | "yes"));
    let cmd = if after {
        format!("media delete-past-remote-media {duration} --after")
    } else {
        format!("media delete-past-remote-media {duration}")
    };
    let msg = if after {
        format!("Purged remote media newer than {duration}")
    } else {
        format!("Purged remote media older than {duration}")
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
