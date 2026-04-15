pub mod appservices;
pub mod federation;
pub mod media;
pub mod rooms;
pub mod server;
pub mod tokens;
pub mod users;

use axum::{
    extract::{Form, OriginalUri, Query, Request, State},
    http::{StatusCode, Uri},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::sync::Arc;
use tera::Context;
use tower_sessions::Session;

use crate::{
    commands,
    matrix::{self, server_name_from_mxid},
    server as server_mod, Ctx,
};

const SESS_KEY: &str = "sess";
const FLASH_KEY: &str = "flash";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct Flash {
    kind: String,
    text: String,
    #[serde(default)]
    log: Option<matrix::LogEntry>,
}

pub(super) async fn take_flash(session: &Session) -> Option<Flash> {
    let f: Option<Flash> = session.get(FLASH_KEY).await.ok().flatten();
    if f.is_some() {
        let _ = session.remove::<Flash>(FLASH_KEY).await;
    }
    f
}

pub(super) async fn set_flash(session: &Session, kind: &str, text: impl Into<String>) {
    let _ = session
        .insert(
            FLASH_KEY,
            &Flash {
                kind: kind.into(),
                text: text.into(),
                log: None,
            },
        )
        .await;
}

async fn set_flash_with_log(
    session: &Session,
    kind: &str,
    text: impl Into<String>,
    log: matrix::LogEntry,
) {
    let _ = session
        .insert(
            FLASH_KEY,
            &Flash {
                kind: kind.into(),
                text: text.into(),
                log: Some(log),
            },
        )
        .await;
}

pub(super) fn insert_flash(ctx: &mut Context, flash: Option<Flash>) {
    if let Some(f) = flash {
        ctx.insert("flash", &f);
    }
}

// Sanitize a ?next= target: only allow single-slash absolute paths.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") && !n.starts_with("/\\") => {
            n.to_string()
        }
        _ => "/".to_string(),
    }
}

// Build a redirect to the login page, preserving the current URI as ?next=.
fn redirect_to_login(uri: &Uri) -> Response {
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    if path == "/" || path.is_empty() || path == "/login" {
        Redirect::to("/login").into_response()
    } else {
        Redirect::to(&format!("/login?next={}", urlencoding::encode(path))).into_response()
    }
}

#[derive(Deserialize, Default)]
pub struct NextQuery {
    #[serde(default)]
    pub next: Option<String>,
}

// Parse an HTML form checkbox value into a bool. Browsers send "on" when checked,
// but we also accept a few other truthy spellings for robustness.
pub(super) fn checkbox(val: Option<&str>) -> bool {
    matches!(val, Some("on" | "true" | "1" | "yes"))
}

// Split a textarea value into non-empty trimmed lines.
pub(super) fn split_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

// Build a redirect response to the given path.
pub(super) fn redirect(to: &str) -> Response {
    Redirect::to(to).into_response()
}

// Flash an error, then redirect. Shorthand for the "missing required field" pattern.
pub(super) async fn redirect_with_err(session: &Session, msg: &str, to: &str) -> Response {
    set_flash(session, "error", msg).await;
    redirect(to)
}

// Wrap an admin command body in a fenced code block, appended after the command line.
// Used by commands that accept a list payload (ban-list, deactivate-all, delete-list, ...).
pub(super) fn with_fenced_payload(cmd: &str, payload: &str) -> String {
    format!("{cmd}\n```\n{payload}\n```")
}

// Render the login page.
pub async fn login_page(State(st): State<Arc<Ctx>>, Query(q): Query<NextQuery>) -> Response {
    let next = safe_next(q.next.as_deref());
    render(&st, "login.html", &login_ctx(&st, None, &next))
}

// Wrap the context.
fn login_ctx(st: &Ctx, error: Option<&str>, next: &str) -> Context {
    let mut ctx = Context::new();
    ctx.insert("homeserver", st.matrix.homeserver());
    ctx.insert("next", next);
    if let Some(e) = error {
        ctx.insert("error", e);
    }

    ctx
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

// Handle login form submission.
pub async fn login_submit(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Query(q): Query<NextQuery>,
    Form(f): Form<LoginForm>,
) -> Response {
    let next = safe_next(q.next.as_deref());
    match do_login(&st, session, f).await {
        Ok(()) => Redirect::to(&next).into_response(),
        Err(e) => render(
            &st,
            "login.html",
            &login_ctx(&st, Some(&format!("{e:#}")), &next),
        ),
    }
}

// Do the auth and login flow, set the session.
async fn do_login(st: &Ctx, session: Session, f: LoginForm) -> anyhow::Result<()> {
    let login = st
        .matrix
        .login(
            f.username.trim(),
            &f.password,
            &st.config.matrix.device_id,
            &st.config.matrix.device_display_name,
        )
        .await?;

    // Derive or use configured admin room alias.
    let alias = if st.config.matrix.admin_room_alias.is_empty() {
        let server = server_name_from_mxid(&login.user_id)
            .ok_or_else(|| anyhow::anyhow!("invalid mxid: {}", login.user_id))?;
        format!("#admins:{server}")
    } else {
        st.config.matrix.admin_room_alias.clone()
    };

    let admin_room_id = st
        .matrix
        .resolve_alias(&login.access_token, &alias)
        .await
        .map_err(|e| anyhow::anyhow!("resolve admin room {alias}: {e:#}"))?;

    let members = st
        .matrix
        .joined_members(&login.access_token, &admin_room_id)
        .await?;
    if !members.iter().any(|m| m == &login.user_id) {
        anyhow::bail!(
            "user {} is not a member of the admin room {} (not a server admin)",
            login.user_id,
            alias
        );
    }

    let sess = matrix::Session {
        user_id: login.user_id,
        access_token: login.access_token,
        admin_room_id,
    };
    session.insert(SESS_KEY, &sess).await?;
    Ok(())
}

// Delete the session and logout.
pub async fn logout(State(st): State<Arc<Ctx>>, session: Session) -> Response {
    if let Ok(Some(s)) = session.get::<matrix::Session>(SESS_KEY).await {
        let _ = st.matrix.logout(&s.access_token).await;
    }
    let _ = session.flush().await;
    Redirect::to("/login").into_response()
}

// Auth middleware: requires a valid session on protected routes. Redirects to
// /login?next=<uri> otherwise, and injects matrix::Session into the request.
pub async fn require_auth(
    session: Session,
    OriginalUri(uri): OriginalUri,
    mut req: Request,
    next: Next,
) -> Response {
    match session
        .get::<matrix::Session>(SESS_KEY)
        .await
        .ok()
        .flatten()
    {
        Some(sess) => {
            req.extensions_mut().insert(sess);
            next.run(req).await
        }
        None => redirect_to_login(&uri),
    }
}

// Render the dashboard page.
pub async fn index(
    State(st): State<Arc<Ctx>>,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let mut ctx = base_ctx(&st, &sess, "home");
    ctx.insert("modules", commands::MODULES);
    let dash = server_mod::dashboard(&st.matrix, &sess).await;
    ctx.insert("dash", &dash);
    render(&st, "dashboard.html", &ctx)
}

// Wrap the context with common fields.
pub(super) fn base_ctx(st: &Ctx, sess: &matrix::Session, active: &str) -> Context {
    let mut ctx = Context::new();
    ctx.insert("user_id", &sess.user_id);
    ctx.insert("homeserver", st.matrix.homeserver());
    ctx.insert("admin_room_id", &sess.admin_room_id);
    ctx.insert("modules", commands::MODULES);
    ctx.insert("active", active);
    ctx
}

// Render a template and handle errors.
pub(super) fn render(st: &Ctx, template: &str, ctx: &Context) -> Response {
    match st.tera.render(template, ctx) {
        Ok(body) => Html(body).into_response(),
        Err(e) => {
            let mut msg = format!("template error: {e}");
            let mut src = std::error::Error::source(&e);
            while let Some(s) = src {
                msg.push_str(&format!("\n  caused by: {s}"));
                src = s.source();
            }
            tracing::error!("{msg}");
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
    }
}

// Merge the flash's attached log entry (if any) with this page's log, insert
// into ctx, and raise a `log_warning` flag if any entry looks like an error.
pub(super) fn install_log(
    ctx: &mut Context,
    flash: Option<&Flash>,
    mut page_log: Vec<matrix::LogEntry>,
) {
    let from_flash = flash.and_then(|f| f.log.clone());
    let merged = if let Some(entry) = from_flash.clone() {
        let mut out = Vec::with_capacity(page_log.len() + 1);
        out.push(entry);
        out.append(&mut page_log);
        out
    } else {
        page_log
    };
    let any_error = merged.iter().any(|e| e.is_error);
    ctx.insert("log", &merged);
    if any_error {
        ctx.insert("log_has_error", &true);
    }
    if from_flash.is_some() {
        ctx.insert("log_open", &true);
    }
}

// Run an admin command and set a one-shot flash message based on the outcome.
// Tuwunel's bot replies are free-form; treat any reply body starting with "error" as an error.
pub(super) async fn run_and_flash(
    st: &Ctx,
    sess: &matrix::Session,
    session: &Session,
    cmd: &str,
    success: &str,
) {
    match st.matrix.run_admin(sess, cmd).await {
        Ok(r) => {
            let looks_error = matrix::is_error_reply(&r.body);
            let kind = if looks_error { "error" } else { "success" };
            let text = if looks_error {
                format!("Command failed: {cmd}")
            } else {
                success.to_string()
            };
            let log = matrix::LogEntry {
                cmd: cmd.to_string(),
                body: r.body,
                is_error: looks_error,
            };
            set_flash_with_log(session, kind, text, log).await;
        }
        Err(e) => {
            set_flash_with_log(
                session,
                "error",
                format!("{e:#}"),
                matrix::LogEntry {
                    cmd: cmd.to_string(),
                    body: String::new(),
                    is_error: true,
                },
            )
            .await;
        }
    }
}

// Convert markdown to HTML for rendering command replies.
pub(super) fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::with_capacity(md.len() * 2);
    html::push_html(&mut out, parser);
    out
}
