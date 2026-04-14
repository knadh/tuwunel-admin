use axum::{
    extract::{Form, Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, insert_flash, install_log, markdown_to_html, render, run_and_flash, set_flash,
    take_flash,
};
use crate::{matrix, parse, rooms, Ctx};

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub exclude_banned: Option<String>,
    #[serde(default)]
    pub exclude_disabled: Option<String>,
}

#[derive(Deserialize)]
pub struct AliasLookupQuery {
    pub alias: String,
}

#[derive(Deserialize)]
pub struct AliasForm {
    pub localpart: String,
    #[serde(default)]
    pub force: Option<String>,
}

#[derive(Deserialize)]
pub struct PruneForm {
    #[serde(default)]
    pub force: Option<String>,
}

#[derive(Deserialize)]
pub struct BanListForm {
    pub rooms: String,
}

#[derive(Deserialize)]
pub struct ForceJoinUsersForm {
    #[serde(default)]
    pub mxids: Option<String>,
    #[serde(default)]
    pub all: Option<String>,
    #[serde(default)]
    pub confirm: Option<String>,
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
        exclude_banned: matches!(
            q.exclude_banned.as_deref(),
            Some("on" | "true" | "1" | "yes")
        ),
        exclude_disabled: matches!(
            q.exclude_disabled.as_deref(),
            Some("on" | "true" | "1" | "yes")
        ),
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
    let alias = q.alias.trim().trim_start_matches('#');
    let localpart = alias.split(':').next().unwrap_or(alias);
    if localpart.is_empty() {
        set_flash(&session, "error", "Alias is required.").await;
        return Redirect::to("/rooms").into_response();
    }
    let cmd = format!("rooms alias which {localpart}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => {
            if let Some(room_id) = parse::alias_resolves_to(&r.body) {
                Redirect::to(&format!("/rooms/{}", urlencoding::encode(&room_id))).into_response()
            } else {
                set_flash(
                    &session,
                    "error",
                    format!("Alias #{localpart} not found on this server."),
                )
                .await;
                Redirect::to("/rooms").into_response()
            }
        }
        Err(e) => {
            set_flash(&session, "error", format!("{e:#}")).await;
            Redirect::to("/rooms").into_response()
        }
    }
}

pub async fn prune_empty(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<PruneForm>,
) -> Response {
    let cmd = if matches!(f.force.as_deref(), Some("on" | "true" | "1" | "yes")) {
        "rooms prune-empty --force"
    } else {
        "rooms prune-empty"
    };
    run_and_flash(&st, &sess, &session, cmd, "Pruned empty rooms").await;
    Redirect::to("/rooms").into_response()
}

pub async fn ban_list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<BanListForm>,
) -> Response {
    let ids: Vec<&str> = f
        .rooms
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if ids.is_empty() {
        set_flash(&session, "error", "Select at least one room.").await;
        return Redirect::to("/rooms").into_response();
    }
    let mut cmd = String::from("rooms moderation ban-list-of-rooms\n```\n");
    cmd.push_str(&ids.join("\n"));
    cmd.push_str("\n```");
    let n = ids.len();
    run_and_flash(&st, &sess, &session, &cmd, &format!("Banned {n} room(s)")).await;
    Redirect::to("/rooms").into_response()
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

pub async fn directory_publish(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms directory publish {room_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Published {room_id} to directory"),
    )
    .await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn directory_unpublish(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms directory unpublish {room_id}");
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Unpublished {room_id}"),
    )
    .await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn alias_add(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<AliasForm>,
) -> Response {
    let lp = f.localpart.trim().trim_start_matches('#');
    let lp = lp.split(':').next().unwrap_or(lp);
    if lp.is_empty() {
        set_flash(&session, "error", "Alias localpart is required.").await;
        return Redirect::to(&format!("/rooms/{room_id}")).into_response();
    }
    let cmd = if matches!(f.force.as_deref(), Some("on" | "true" | "1" | "yes")) {
        format!("rooms alias set --force {room_id} {lp}")
    } else {
        format!("rooms alias set {room_id} {lp}")
    };
    run_and_flash(&st, &sess, &session, &cmd, &format!("Added alias #{lp}")).await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
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
    let lp = f.localpart.trim().trim_start_matches('#');
    let lp = lp.split(':').next().unwrap_or(lp);
    if lp.is_empty() {
        set_flash(&session, "error", "Alias localpart is required.").await;
        return Redirect::to(&format!("/rooms/{room_id}")).into_response();
    }
    let cmd = format!("rooms alias remove {lp}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Removed alias #{lp}")).await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn force_join_users(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
    Form(f): Form<ForceJoinUsersForm>,
) -> Response {
    if !matches!(f.confirm.as_deref(), Some("on" | "true" | "1" | "yes")) {
        set_flash(
            &session,
            "error",
            "You must explicitly confirm this destructive action.",
        )
        .await;
        return Redirect::to(&format!("/rooms/{room_id}")).into_response();
    }
    let all = matches!(f.all.as_deref(), Some("on" | "true" | "1" | "yes"));
    let cmd = if all {
        format!("users force-join-all-local-users --yes-i-want-to-do-this {room_id}")
    } else {
        let mxids: Vec<&str> = f
            .mxids
            .as_deref()
            .unwrap_or("")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if mxids.is_empty() {
            set_flash(&session, "error", "Provide a list of users.").await;
            return Redirect::to(&format!("/rooms/{room_id}")).into_response();
        }
        let mut cmd = format!(
            "users force-join-list-of-local-users --yes-i-want-to-do-this {room_id}\n```\n"
        );
        cmd.push_str(&mxids.join("\n"));
        cmd.push_str("\n```");
        cmd
    };
    let msg = if all {
        format!("Force-joined all local users to {room_id}")
    } else {
        format!("Force-joined users to {room_id}")
    };
    run_and_flash(&st, &sess, &session, &cmd, &msg).await;
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
