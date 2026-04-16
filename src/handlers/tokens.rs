use axum::{
    extract::{Form, Path, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, checkbox, cmd_flag, insert_flash, install_log, redirect_with_err, render,
    run_and_redirect, take_flash,
};
use crate::{matrix, tokens, Ctx};

#[derive(Deserialize)]
pub struct IssueTokenForm {
    pub max_uses: Option<String>,
    pub max_age: Option<String>,
    pub once: Option<String>,
}

pub async fn list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "tokens");
    match tokens::list(&st.matrix, &sess).await {
        Ok((rows, log)) => {
            ctx.insert("tokens", &rows);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "tokens/list.html", &ctx)
}

pub async fn issue(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<IssueTokenForm>,
) -> Response {
    let mut cmd = String::from("token issue");
    cmd_flag(&mut cmd, "max-uses", f.max_uses.as_ref());
    cmd_flag(&mut cmd, "max-age", f.max_age.as_ref());
    if checkbox(f.once.as_deref()) {
        cmd.push_str(" --once");
    }
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        "Issued new registration token",
        "/tokens",
    )
    .await
}

pub async fn revoke(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(token): Path<String>,
) -> Response {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return redirect_with_err(&session, "Token is required.", "/tokens").await;
    }
    let cmd = format!("token revoke {trimmed}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Revoked token {trimmed}"),
        "/tokens",
    )
    .await
}
