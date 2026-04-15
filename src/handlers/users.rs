use axum::{
    extract::{Form, Path, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, checkbox, redirect_with_err, insert_flash, install_log, markdown_to_html,
    redirect, render, run_and_flash, set_flash, split_lines, take_flash, with_fenced_payload,
};
use crate::{matrix, users, Ctx};

#[derive(Deserialize)]
pub struct CreateUserForm {
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub generate: Option<String>,
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

#[derive(Deserialize)]
pub struct DeactivateForm {
    #[serde(default)]
    pub no_leave_rooms: Option<String>,
}

#[derive(Deserialize)]
pub struct DeactivateAllForm {
    pub mxids: String,
    #[serde(default)]
    pub no_leave_rooms: Option<String>,
    #[serde(default)]
    pub force: Option<String>,
}

#[derive(Deserialize)]
pub struct TagForm {
    pub verb: String,
    pub tag: String,
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
    if username.is_empty() {
        return redirect_with_err(&session, "Username is required.", "/users").await;
    }
    let generate = checkbox(f.generate.as_deref());
    let password = f.password.as_deref().unwrap_or("").trim();
    let autogen = generate || password.is_empty();
    let cmd = if autogen {
        format!("users create-user {username}")
    } else {
        format!("users create-user {username} {password}")
    };

    if autogen {
        // Capture the bot's reply so the generated password lands in the flash.
        match st.matrix.run_admin(&sess, &cmd).await {
            Ok(r) if !matrix::is_error_reply(&r.body) => {
                set_flash(
                    &session,
                    "success",
                    format!("Created user {username}.\n\n{}", r.body.trim()),
                )
                .await;
            }
            Ok(r) => {
                set_flash(
                    &session,
                    "error",
                    format!("Create failed: {}", r.body.trim()),
                )
                .await;
            }
            Err(e) => {
                set_flash(&session, "error", format!("{e:#}")).await;
            }
        }
    } else {
        run_and_flash(
            &st,
            &sess,
            &session,
            &cmd,
            &format!("Created user {username}"),
        )
        .await;
    }
    redirect("/users")
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
            let log = d.log.clone();
            ctx.insert("detail", &d);
            ctx.insert("joined_rooms_html", &raw_html);
            install_log(&mut ctx, flash.as_ref(), log);
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
    let back = format!("/users/{mxid}");
    if f.password.is_empty() {
        return redirect_with_err(&session, "Password is required.", &back).await;
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
    redirect(&back)
}

pub async fn deactivate(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<DeactivateForm>,
) -> Response {
    let cmd = if checkbox(f.no_leave_rooms.as_deref()) {
        format!("users deactivate --no-leave-rooms {mxid}")
    } else {
        format!("users deactivate {mxid}")
    };
    run_and_flash(&st, &sess, &session, &cmd, &format!("Deactivated {mxid}")).await;
    redirect("/users")
}

pub async fn deactivate_all(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<DeactivateAllForm>,
) -> Response {
    let mxids = split_lines(&f.mxids);
    if mxids.is_empty() {
        return redirect_with_err(&session, "Select at least one user.", "/users").await;
    }
    let mut head = String::from("users deactivate-all");
    if checkbox(f.no_leave_rooms.as_deref()) {
        head.push_str(" --no-leave-rooms");
    }
    if checkbox(f.force.as_deref()) {
        head.push_str(" --force");
    }
    let n = mxids.len();
    let cmd = with_fenced_payload(&head, &mxids.join("\n"));
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deactivated {n} user(s)"),
    )
    .await;
    redirect("/users")
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
    redirect(&format!("/users/{mxid}"))
}

pub async fn force_join(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<RoomForm>,
) -> Response {
    let back = format!("/users/{mxid}");
    let room = f.room.trim();
    if room.is_empty() {
        return redirect_with_err(&session, "Room is required.", &back).await;
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
    redirect(&back)
}

pub async fn force_leave(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<RoomIdForm>,
) -> Response {
    let back = format!("/users/{mxid}");
    let room = f.room_id.trim();
    if room.is_empty() {
        return redirect_with_err(&session, "Room ID is required.", &back).await;
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
    redirect(&back)
}

pub async fn redact_event(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<EventIdForm>,
) -> Response {
    let back = format!("/users/{mxid}");
    let evt = f.event_id.trim();
    if evt.is_empty() {
        return redirect_with_err(&session, "Event ID is required.", &back).await;
    }
    let cmd = format!("users redact-event {evt}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Redacted {evt}")).await;
    redirect(&back)
}

pub async fn delete_device(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path((mxid, device_id)): Path<(String, String)>,
) -> Response {
    let cmd = format!("users delete-device {mxid} {device_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted device {device_id}"),
    )
    .await;
    redirect(&format!("/users/{mxid}"))
}

pub async fn force_promote(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path((mxid, room_id)): Path<(String, String)>,
) -> Response {
    let cmd = format!("users force-promote {mxid} {room_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Promoted {mxid} in {room_id}"),
    )
    .await;
    redirect(&format!("/users/{mxid}"))
}

pub async fn force_demote(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path((mxid, room_id)): Path<(String, String)>,
) -> Response {
    let cmd = format!("users force-demote {mxid} {room_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Demoted {mxid} in {room_id}"),
    )
    .await;
    redirect(&format!("/users/{mxid}"))
}

pub async fn room_tag(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path((mxid, room_id)): Path<(String, String)>,
    Form(f): Form<TagForm>,
) -> Response {
    let back = format!("/users/{mxid}");
    let tag = f.tag.trim();
    if tag.is_empty() {
        return redirect_with_err(&session, "Tag is required.", &back).await;
    }
    let (cmd, msg) = if f.verb == "delete" {
        (
            format!("users delete-room-tag {mxid} {room_id} {tag}"),
            format!("Removed tag {tag} from {room_id}"),
        )
    } else {
        (
            format!("users put-room-tag {mxid} {room_id} {tag}"),
            format!("Tagged {room_id} as {tag}"),
        )
    };
    run_and_flash(&st, &sess, &session, &cmd, &msg).await;
    redirect(&back)
}

pub async fn get_room_tags(
    State(st): State<Arc<Ctx>>,
    Extension(sess): Extension<matrix::Session>,
    Path((mxid, room_id)): Path<(String, String)>,
) -> Response {
    let cmd = format!("users get-room-tags {mxid} {room_id}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => Json(json!({ "body": r.body })).into_response(),
        Err(e) => Json(json!({ "error": format!("{e:#}") })).into_response(),
    }
}
