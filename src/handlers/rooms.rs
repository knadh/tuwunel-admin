use axum::{
    extract::{Form, Path, Query, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, checkbox, insert_flash, install_log, markdown_to_html, redirect, redirect_with_err,
    render, run_and_redirect, split_lines, take_flash, validate_line, with_fenced_payload,
};
use crate::{matrix, parse, rooms, Ctx};

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub page: Option<u32>,
    pub exclude_banned: Option<String>,
    pub exclude_disabled: Option<String>,
}

#[derive(Deserialize)]
pub struct AliasLookupQuery {
    pub alias: String,
}

#[derive(Deserialize)]
pub struct AliasForm {
    pub localpart: String,
    pub force: Option<String>,
}

#[derive(Deserialize)]
pub struct PruneForm {
    pub force: Option<String>,
}

#[derive(Deserialize)]
pub struct BanListForm {
    pub rooms: String,
}

#[derive(Deserialize)]
pub struct ForceJoinUsersForm {
    pub mxids: Option<String>,
    pub all: Option<String>,
    pub confirm: Option<String>,
}

// Normalize a user-submitted alias down to its localpart: strip whitespace,
// a leading `#`, and anything from the `:server` suffix onward.
fn alias_localpart(raw: &str) -> &str {
    let s = raw.trim().trim_start_matches('#');
    s.split(':').next().unwrap_or(s)
}

pub async fn list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Query(q): Query<ListQuery>,
) -> Response {
    let flash = take_flash(&session).await;

    let opts = rooms::ListOpts {
        page: q.page,
        exclude_banned: checkbox(q.exclude_banned.as_deref()),
        exclude_disabled: checkbox(q.exclude_disabled.as_deref()),
    };

    let mut ctx = base_ctx(&st, &sess, "rooms");
    ctx.insert("page", &opts.page.unwrap_or(1));
    ctx.insert("exclude_banned", &opts.exclude_banned);
    ctx.insert("exclude_disabled", &opts.exclude_disabled);
    match rooms::list(&st.matrix, &sess, &opts).await {
        Ok((rows, log)) => {
            ctx.insert("rooms", &rows);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "rooms/list.html", &ctx)
}

pub async fn find_by_alias(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Query(q): Query<AliasLookupQuery>,
) -> Response {
    let localpart = alias_localpart(&q.alias);
    if localpart.is_empty() {
        return redirect_with_err(&session, "Alias is required.", "/rooms").await;
    }
    let cmd = format!("rooms alias which {localpart}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => match parse::alias_resolves_to(&r.body) {
            Some(room_id) => redirect(&format!("/rooms/{}", urlencoding::encode(&room_id))),
            None => {
                redirect_with_err(
                    &session,
                    &format!("Alias #{localpart} not found on this server."),
                    "/rooms",
                )
                .await
            }
        },
        Err(e) => redirect_with_err(&session, &format!("{e:#}"), "/rooms").await,
    }
}

pub async fn prune_empty(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<PruneForm>,
) -> Response {
    let cmd = if checkbox(f.force.as_deref()) {
        "rooms prune-empty --force"
    } else {
        "rooms prune-empty"
    };
    run_and_redirect(&st, &sess, &session, cmd, "Pruned empty rooms", "/rooms").await
}

pub async fn ban_list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<BanListForm>,
) -> Response {
    let ids = split_lines(&f.rooms);
    if ids.is_empty() {
        return redirect_with_err(&session, "Select at least one room.", "/rooms").await;
    }
    let n = ids.len();
    let cmd = with_fenced_payload("rooms moderation ban-list-of-rooms", &ids.join("\n"));
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Banned {n} room(s)"),
        "/rooms",
    )
    .await
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

// Run `cmd`, flash, and redirect to the room detail page.
async fn run_on_room(
    st: &Ctx,
    sess: &matrix::Session,
    session: &Session,
    room_id: &str,
    cmd: &str,
    success: &str,
) -> Response {
    run_and_redirect(
        st,
        sess,
        session,
        cmd,
        success,
        &format!("/rooms/{room_id}"),
    )
    .await
}

pub async fn ban(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms moderation ban-room {room_id}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Banned {room_id}"),
    )
    .await
}

pub async fn unban(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms moderation unban-room {room_id}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Unbanned {room_id}"),
    )
    .await
}

pub async fn federation_enable(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("federation enable-room {room_id}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Enabled federation for {room_id}"),
    )
    .await
}

pub async fn federation_disable(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("federation disable-room {room_id}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Disabled federation for {room_id}"),
    )
    .await
}

pub async fn directory_publish(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms directory publish {room_id}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Published {room_id} to directory"),
    )
    .await
}

pub async fn directory_unpublish(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms directory unpublish {room_id}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Unpublished {room_id}"),
    )
    .await
}

pub async fn alias_add(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<AliasForm>,
) -> Response {
    let back = format!("/rooms/{room_id}");
    let lp = alias_localpart(&f.localpart);
    if lp.is_empty() {
        return redirect_with_err(&session, "Alias localpart is required.", &back).await;
    }
    if !validate_line(lp) {
        return redirect_with_err(&session, "Alias cannot contain line breaks.", &back).await;
    }
    let cmd = if checkbox(f.force.as_deref()) {
        format!("rooms alias set --force {room_id} {lp}")
    } else {
        format!("rooms alias set {room_id} {lp}")
    };
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Added alias #{lp}"),
    )
    .await
}

#[derive(Deserialize)]
pub struct AliasRemoveForm {
    pub localpart: String,
}

pub async fn alias_remove(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<AliasRemoveForm>,
) -> Response {
    let back = format!("/rooms/{room_id}");
    let lp = alias_localpart(&f.localpart);
    if lp.is_empty() {
        return redirect_with_err(&session, "Alias localpart is required.", &back).await;
    }
    if !validate_line(lp) {
        return redirect_with_err(&session, "Alias cannot contain line breaks.", &back).await;
    }
    let cmd = format!("rooms alias remove {lp}");
    run_on_room(
        &st,
        &sess,
        &session,
        &room_id,
        &cmd,
        &format!("Removed alias #{lp}"),
    )
    .await
}

pub async fn force_join_users(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<ForceJoinUsersForm>,
) -> Response {
    let back = format!("/rooms/{room_id}");
    if !checkbox(f.confirm.as_deref()) {
        return redirect_with_err(
            &session,
            "You must explicitly confirm this destructive action.",
            &back,
        )
        .await;
    }
    let all = checkbox(f.all.as_deref());
    let (cmd, msg) = if all {
        (
            format!("users force-join-all-local-users --yes-i-want-to-do-this {room_id}"),
            format!("Force-joined all local users to {room_id}"),
        )
    } else {
        let mxids = split_lines(f.mxids.as_deref().unwrap_or(""));
        if mxids.is_empty() {
            return redirect_with_err(&session, "Provide a list of users.", &back).await;
        }
        (
            with_fenced_payload(
                &format!("users force-join-list-of-local-users --yes-i-want-to-do-this {room_id}"),
                &mxids.join("\n"),
            ),
            format!("Force-joined users to {room_id}"),
        )
    };
    run_on_room(&st, &sess, &session, &room_id, &cmd, &msg).await
}

#[derive(Deserialize)]
pub struct DeleteRoomForm {
    pub force: Option<String>,
}

pub async fn delete(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<DeleteRoomForm>,
) -> Response {
    let cmd = if checkbox(f.force.as_deref()) {
        format!("rooms delete {room_id} --force")
    } else {
        format!("rooms delete {room_id}")
    };
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Deleted {room_id}"),
        "/rooms",
    )
    .await
}
