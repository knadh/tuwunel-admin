use axum::{
    extract::{Form, Path, State},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, insert_flash, install_log, markdown_to_html, render, run_and_flash, take_flash,
};
use crate::{matrix, rooms, Ctx};

pub async fn list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "rooms");
    match rooms::list(&st.matrix, &sess).await {
        Ok((rows, log)) => {
            ctx.insert("rooms", &rows);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "rooms/list.html", &ctx)
}

pub async fn detail(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "rooms");
    ctx.insert("room_id", &room_id);
    match rooms::detail(&st.matrix, &sess, &room_id).await {
        Ok(d) => {
            let raw_html = markdown_to_html(&d.members_raw);
            let log = d.log.clone();
            ctx.insert("detail", &d);
            ctx.insert("members_html", &raw_html);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "rooms/detail.html", &ctx)
}

pub async fn ban(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms moderation ban-room {room_id}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Banned {room_id}")).await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn unban(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms moderation unban-room {room_id}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Unbanned {room_id}")).await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn federation_enable(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("federation enable-room {room_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Enabled federation for {room_id}"),
    )
    .await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn federation_disable(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("federation disable-room {room_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Disabled federation for {room_id}"),
    )
    .await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

#[derive(Deserialize)]
pub struct DeleteRoomForm {
    #[serde(default)]
    pub force: Option<String>,
}

pub async fn delete(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<DeleteRoomForm>,
) -> Response {
    let cmd = if f.force.as_deref().is_some_and(|v| !v.is_empty()) {
        format!("rooms delete {room_id} --force")
    } else {
        format!("rooms delete {room_id}")
    };
    run_and_flash(&st, &sess, &session, &cmd, &format!("Deleted {room_id}")).await;
    Redirect::to("/rooms").into_response()
}
