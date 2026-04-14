use axum::{
    extract::{Form, Path, State},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, insert_flash, markdown_to_html, render, run_and_flash, set_flash, take_flash,
};
use crate::{matrix, users, Ctx};

#[derive(Deserialize)]
pub struct CreateUserForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub password: String,
}

#[derive(Deserialize)]
pub struct RoomForm {
    pub room: String,
}

#[derive(Deserialize)]
pub struct RoomIdForm {
    pub room_id: String,
}

#[derive(Deserialize)]
pub struct EventIdForm {
    pub event_id: String,
}

pub async fn list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "users");
    match users::list(&st.matrix, &sess).await {
        Ok(rows) => ctx.insert("users", &rows),
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "users/list.html", &ctx)
}

pub async fn create(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<CreateUserForm>,
) -> Response {
    let username = f.username.trim();
    if username.is_empty() || f.password.is_empty() {
        set_flash(&session, "error", "Username and password are required.").await;
        return Redirect::to("/users").into_response();
    }
    let cmd = format!("users create-user {username} {}", f.password);
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Created user {username}"),
    )
    .await;
    Redirect::to("/users").into_response()
}

pub async fn detail(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "users");
    ctx.insert("mxid", &mxid);
    match users::detail(&st.matrix, &sess, &mxid).await {
        Ok(d) => {
            let raw_html = markdown_to_html(&d.joined_rooms_raw);
            ctx.insert("detail", &d);
            ctx.insert("joined_rooms_html", &raw_html);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "users/detail.html", &ctx)
}

pub async fn reset_password(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<PasswordForm>,
) -> Response {
    if f.password.is_empty() {
        set_flash(&session, "error", "Password is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
    }
    let cmd = format!("users reset-password {mxid} {}", f.password);
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Reset password for {mxid}"),
    )
    .await;
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn deactivate(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
) -> Response {
    let cmd = format!("users deactivate {mxid}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Deactivated {mxid}")).await;
    Redirect::to("/users").into_response()
}

pub async fn make_admin(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
) -> Response {
    let cmd = format!("users make-user-admin {mxid}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Granted admin to {mxid}"),
    )
    .await;
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn force_join(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<RoomForm>,
) -> Response {
    let room = f.room.trim();
    if room.is_empty() {
        set_flash(&session, "error", "Room is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
    }
    let cmd = format!("users force-join-room {mxid} {room}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Joined {mxid} to {room}"),
    )
    .await;
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn force_leave(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<RoomIdForm>,
) -> Response {
    let room = f.room_id.trim();
    if room.is_empty() {
        set_flash(&session, "error", "Room ID is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
    }
    let cmd = format!("users force-leave-room {mxid} {room}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Removed {mxid} from {room}"),
    )
    .await;
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn redact_event(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<EventIdForm>,
) -> Response {
    let evt = f.event_id.trim();
    if evt.is_empty() {
        set_flash(&session, "error", "Event ID is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
    }
    let cmd = format!("users redact-event {evt}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Redacted {evt}")).await;
    Redirect::to(&format!("/users/{mxid}")).into_response()
}
