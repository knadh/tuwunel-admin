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

use crate::{commands, matrix, server as server_mod, Ctx};

const SESS_KEY: &str = "sess";
const FLASH_KEY: &str = "flash";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct Flash {
    kind: String,
    text: String,
    log: Option<matrix::LogEntry>,
}

pub(super) async fn take_flash(session: &Session) -> Option<Flash> {
    let f: Option<Flash> = session.get(FLASH_KEY).await.ok().flatten();
    if f.is_some() {
        let _ = session.remove::<Flash>(FLASH_KEY).await;
    }
    f
}

async fn write_flash(session: &Session, kind: &str, text: String, log: Option<matrix::LogEntry>) {
    let _ = session
        .insert(
            FLASH_KEY,
            &Flash {
                kind: kind.into(),
                text,
                log,
            },
        )
        .await;
}

pub(super) async fn set_flash(session: &Session, kind: &str, text: impl Into<String>) {
    write_flash(session, kind, text.into(), None).await;
}

async fn set_flash_with_log(
    session: &Session,
    kind: &str,
    text: impl Into<String>,
    log: matrix::LogEntry,
) {
    write_flash(session, kind, text.into(), Some(log)).await;
}

pub(super) fn insert_flash(ctx: &mut Context, flash: Option<Flash>) {
    if let Some(f) = flash {
        ctx.insert("flash", &f);
    }
}

// Only allow single-slash absolute paths to defeat open-redirect.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") && !n.starts_with("/\\") => {
            n.to_string()
        }
        _ => "/".to_string(),
    }
}

fn redirect_to_login(uri: &Uri) -> Response {
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    if matches!(path, "/" | "/login") {
        Redirect::to("/login").into_response()
    } else {
        Redirect::to(&format!("/login?next={}", urlencoding::encode(path))).into_response()
    }
}

#[derive(Deserialize, Default)]
pub struct NextQuery {
    pub next: Option<String>,
}

pub(super) fn checkbox(val: Option<&str>) -> bool {
    val == Some("on")
}

pub(super) fn split_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

pub(super) fn redirect(to: &str) -> Response {
    Redirect::to(to).into_response()
}

pub(super) async fn redirect_with_err(session: &Session, msg: &str, to: &str) -> Response {
    set_flash(session, "error", msg).await;
    redirect(to)
}

// Append a fenced payload after the command line. Used by list-payload commands
// like `ban-list`, `deactivate-all`, `delete-list`.
pub(super) fn with_fenced_payload(cmd: &str, payload: &str) -> String {
    format!("{cmd}\n```\n{payload}\n```")
}

pub(super) fn cmd_flag(cmd: &mut String, name: &str, val: Option<&String>) {
    if let Some(val) = val.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmd.push_str(" --");
        cmd.push_str(name);
        cmd.push(' ');
        cmd.push_str(val);
    }
}

fn configured_homeservers(st: &Ctx) -> Vec<String> {
    st.config
        .matrix
        .homeservers
        .iter()
        .map(|h| matrix::normalize(h))
        .filter(|h| !h.is_empty())
        .collect()
}

pub async fn login_page(State(st): State<Arc<Ctx>>, Query(q): Query<NextQuery>) -> Response {
    let next = safe_next(q.next.as_deref());
    render(&st, "login.html", &login_ctx(&st, None, &next))
}

fn login_ctx(st: &Ctx, error: Option<&str>, next: &str) -> Context {
    let mut ctx = Context::new();
    let homeservers = configured_homeservers(st);
    let allow_any = st.config.matrix.allow_any_server;
    ctx.insert("allow_any_server", &allow_any);
    ctx.insert("next", next);
    if homeservers.is_empty() && !allow_any {
        ctx.insert(
            "config_error",
            "No homeservers are configured. Set [matrix].homeservers or allow_any_server = true.",
        );
    }
    ctx.insert("homeservers", &homeservers);
    if let Some(e) = error {
        ctx.insert("error", e);
    }

    ctx
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub homeserver: String,
}

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

async fn do_login(st: &Ctx, session: Session, f: LoginForm) -> anyhow::Result<()> {
    let homeserver = pick_homeserver(st, &f)?;

    let login = st
        .matrix
        .login(
            &homeserver,
            f.username.trim(),
            &f.password,
            &st.config.matrix.device_id,
            &st.config.matrix.device_display_name,
        )
        .await?;

    let alias = if st.config.matrix.admin_room_alias.is_empty() {
        let (_, server) = login
            .user_id
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid mxid: {}", login.user_id))?;
        format!("#admins:{server}")
    } else {
        st.config.matrix.admin_room_alias.clone()
    };

    let admin_room_id = st
        .matrix
        .resolve_alias(&homeserver, &login.access_token, &alias)
        .await
        .map_err(|e| anyhow::anyhow!("resolve admin room {alias}: {e:#}"))?;

    let members = st
        .matrix
        .joined_members(&homeserver, &login.access_token, &admin_room_id)
        .await?;
    if !members.contains(&login.user_id) {
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
        homeserver,
    };
    session.insert(SESS_KEY, &sess).await?;
    Ok(())
}

fn pick_homeserver(st: &Ctx, f: &LoginForm) -> anyhow::Result<String> {
    let configured = configured_homeservers(st);
    let allow_any = st.config.matrix.allow_any_server;

    if configured.is_empty() && !allow_any {
        anyhow::bail!("no homeservers are configured");
    }

    let submitted = matrix::normalize(&f.homeserver);
    if submitted.is_empty() {
        anyhow::bail!("select or enter a homeserver URL");
    }
    if !(submitted.starts_with("http://") || submitted.starts_with("https://")) {
        anyhow::bail!("homeserver URL must start with http:// or https://");
    }
    if !allow_any && !configured.iter().any(|h| h == &submitted) {
        anyhow::bail!("homeserver is not in the allowed list");
    }
    Ok(submitted)
}

pub async fn logout(State(st): State<Arc<Ctx>>, session: Session) -> Response {
    if let Ok(Some(s)) = session.get::<matrix::Session>(SESS_KEY).await {
        let _ = st.matrix.logout(&s.homeserver, &s.access_token).await;
    }
    let _ = session.flush().await;
    Redirect::to("/login").into_response()
}

// Injects matrix::Session into the request; redirects to /login?next=<uri> if absent.
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

pub async fn index(
    State(st): State<Arc<Ctx>>,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let mut ctx = base_ctx(&st, &sess, "home");
    let dash = server_mod::dashboard(&st.matrix, &sess).await;
    ctx.insert("dash", &dash);
    render(&st, "dashboard.html", &ctx)
}

pub(super) fn base_ctx(_st: &Ctx, sess: &matrix::Session, active: &str) -> Context {
    let mut ctx = Context::new();
    ctx.insert("user_id", &sess.user_id);
    ctx.insert("homeserver", &sess.homeserver);
    ctx.insert("admin_room_id", &sess.admin_room_id);
    ctx.insert("modules", commands::MODULES);
    ctx.insert("active", active);
    ctx
}

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

// Prepend the flash's log entry (if any) to this page's log and insert into ctx.
pub(super) fn install_log(
    ctx: &mut Context,
    flash: Option<&Flash>,
    mut page_log: Vec<matrix::LogEntry>,
) {
    if let Some(entry) = flash.and_then(|f| f.log.clone()) {
        page_log.insert(0, entry);
        ctx.insert("log_open", &true);
    }
    if page_log.iter().any(|e| e.is_error) {
        ctx.insert("log_has_error", &true);
    }
    ctx.insert("log", &page_log);
}

// Tuwunel's bot replies are free-form text, not status codes, so we sniff for
// failure by body content (see `is_error_reply`) rather than a reply type.
pub(super) async fn run_and_flash(
    st: &Ctx,
    sess: &matrix::Session,
    session: &Session,
    cmd: &str,
    success: &str,
) {
    let (kind, text, body, is_error) = match st.matrix.run_admin(sess, cmd).await {
        Ok(r) if matrix::is_error_reply(&r.body) => {
            ("error", format!("Command failed: {cmd}"), r.body, true)
        }
        Ok(r) => ("success", success.to_string(), r.body, false),
        Err(e) => ("error", format!("{e:#}"), String::new(), true),
    };
    let log = matrix::LogEntry {
        cmd: cmd.to_string(),
        body,
        is_error,
    };
    set_flash_with_log(session, kind, text, log).await;
}

pub(super) async fn run_and_redirect(
    st: &Ctx,
    sess: &matrix::Session,
    session: &Session,
    cmd: &str,
    success: &str,
    to: &str,
) -> Response {
    run_and_flash(st, sess, session, cmd, success).await;
    redirect(to)
}

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
