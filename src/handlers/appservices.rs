use axum::{
    extract::{Form, Path, State},
    response::Response,
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{
    base_ctx, insert_flash, install_log, redirect_with_err, render, run_and_redirect, take_flash,
};
use crate::{appservices, matrix, Ctx};

#[derive(Deserialize)]
pub struct RegisterAppserviceForm {
    pub yaml: String,
}

pub async fn list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "appservice");
    match appservices::list(&st.matrix, &sess).await {
        Ok((rows, log)) => {
            ctx.insert("appservices", &rows);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "appservices/list.html", &ctx)
}

pub async fn detail(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(id): Path<String>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "appservice");
    ctx.insert("id", &id);
    match appservices::detail(&st.matrix, &sess, &id).await {
        Ok(d) => {
            let log = d.log.clone();
            ctx.insert("detail", &d);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "appservices/detail.html", &ctx)
}

pub async fn register(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<RegisterAppserviceForm>,
) -> Response {
    let yaml = f.yaml.trim();
    if yaml.is_empty() {
        return redirect_with_err(&session, "Registration YAML is required.", "/appservices").await;
    }
    let cmd = super::with_fenced_payload("appservices register", yaml);
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        "Registered appservice",
        "/appservices",
    )
    .await
}

pub async fn unregister(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(id): Path<String>,
) -> Response {
    let cmd = format!("appservices unregister {id}");
    run_and_redirect(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Unregistered {id}"),
        "/appservices",
    )
    .await
}
