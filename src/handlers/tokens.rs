use axum::{
    extract::{Form, Path, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, checkbox, redirect_with_err, insert_flash, install_log, redirect, render,
    run_and_flash, take_flash,
};
use crate::{matrix, tokens, Ctx};

#[derive(Deserialize)]
pub struct IssueTokenForm {
    #[serde(default)]
    pub max_uses: Option<String>,
    #[serde(default)]
    pub max_age: Option<String>,
    #[serde(default)]
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
    let mut push_flag = |name: &str, v: Option<&String>| {
        if let Some(val) = v.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            cmd.push_str(&format!(" --{name} {val}"));
        }
    };
    push_flag("max-uses", f.max_uses.as_ref());
    push_flag("max-age", f.max_age.as_ref());
    if checkbox(f.once.as_deref()) {
        cmd.push_str(" --once");
    }
    run_and_flash(&st, &sess, &session, &cmd, "Issued new registration token").await;
    redirect("/tokens")
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
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Revoked token {trimmed}"),
    )
    .await;
    redirect("/tokens")
}
