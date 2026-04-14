use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{base_ctx, insert_flash, install_log, markdown_to_html, render, set_flash, take_flash};
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
        set_flash(&session, "error", "Server name is required.").await;
        return Redirect::to("/federation").into_response();
    }
    let cmd = format!("federation fetch-support-well-known {server}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => {
            let html = markdown_to_html(&r.body);
            let mut ctx = base_ctx(&st, &sess, "federation");
            match federation::overview(&st.matrix, &sess).await {
                Ok(o) => {
                    let log = o.log.clone();
                    ctx.insert("overview", &o);
                    install_log(&mut ctx, None, log);
                }
                Err(e) => ctx.insert("error", &format!("{e:#}")),
            }
            ctx.insert("tool_title", &format!("Support info for {server}"));
            ctx.insert("tool_reply_raw", &r.body);
            ctx.insert("tool_reply_html", &html);
            render(&st, "federation/index.html", &ctx)
        }
        Err(e) => {
            set_flash(&session, "error", format!("{e:#}")).await;
            Redirect::to("/federation").into_response()
        }
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
        set_flash(&session, "error", "User ID is required.").await;
        return Redirect::to("/federation").into_response();
    }
    let cmd = format!("federation remote-user-in-rooms {uid}");
    match st.matrix.run_admin(&sess, &cmd).await {
        Ok(r) => {
            let html = markdown_to_html(&r.body);
            let rooms = crate::parse::debug_string_array(&r.body).unwrap_or_default();
            let mut ctx = base_ctx(&st, &sess, "federation");
            match federation::overview(&st.matrix, &sess).await {
                Ok(o) => {
                    let log = o.log.clone();
                    ctx.insert("overview", &o);
                    install_log(&mut ctx, None, log);
                }
                Err(e) => ctx.insert("error", &format!("{e:#}")),
            }
            ctx.insert("tool_title", &format!("Rooms shared with {uid}"));
            ctx.insert("tool_reply_raw", &r.body);
            ctx.insert("tool_reply_html", &html);
            ctx.insert("tool_rooms", &rooms);
            render(&st, "federation/index.html", &ctx)
        }
        Err(e) => {
            set_flash(&session, "error", format!("{e:#}")).await;
            Redirect::to("/federation").into_response()
        }
    }
}
