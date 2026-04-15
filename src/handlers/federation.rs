use axum::{
    extract::{Form, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, insert_flash, install_log, markdown_to_html, redirect_with_err, render, take_flash,
};
use crate::{federation, matrix, Ctx};

#[derive(Deserialize)]
pub struct ServerForm {
    pub server: String,
}

#[derive(Deserialize)]
pub struct UserForm {
    pub user_id: String,
}

pub async fn index(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;
    let mut ctx = base_ctx(&st, &sess, "federation");
    match federation::overview(&st.matrix, &sess).await {
        Ok(o) => {
            let log = o.log.clone();
            ctx.insert("overview", &o);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "federation/index.html", &ctx)
}

pub async fn fetch_well_known(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<ServerForm>,
) -> Response {
    let server = f.server.trim();
    if server.is_empty() {
        return redirect_with_err(&session, "Server name is required.", "/federation").await;
    }
    let cmd = format!("federation fetch-support-well-known {server}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => {
            let mut ctx = overview_ctx(&st, &sess).await;
            ctx.insert("tool_title", &format!("Support info for {server}"));
            ctx.insert("tool_reply_raw", &r.body);
            ctx.insert("tool_reply_html", &markdown_to_html(&r.body));
            render(&st, "federation/index.html", &ctx)
        }
        Err(e) => redirect_with_err(&session, &format!("{e:#}"), "/federation").await,
    }
}

pub async fn remote_user_in_rooms(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<UserForm>,
) -> Response {
    let uid = f.user_id.trim();
    if uid.is_empty() {
        return redirect_with_err(&session, "User ID is required.", "/federation").await;
    }
    let cmd = format!("federation remote-user-in-rooms {uid}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => {
            let rooms = crate::parse::debug_string_array(&r.body).unwrap_or_default();
            let mut ctx = overview_ctx(&st, &sess).await;
            ctx.insert("tool_title", &format!("Rooms shared with {uid}"));
            ctx.insert("tool_reply_raw", &r.body);
            ctx.insert("tool_reply_html", &markdown_to_html(&r.body));
            ctx.insert("tool_rooms", &rooms);
            render(&st, "federation/index.html", &ctx)
        }
        Err(e) => redirect_with_err(&session, &format!("{e:#}"), "/federation").await,
    }
}

// Base ctx + federation overview, shared by the two tool handlers below.
async fn overview_ctx(st: &Ctx, sess: &matrix::Session) -> tera::Context {
    let mut ctx = base_ctx(st, sess, "federation");
    match federation::overview(&st.matrix, sess).await {
        Ok(o) => {
            let log = o.log.clone();
            ctx.insert("overview", &o);
            install_log(&mut ctx, None, log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    ctx
}
