use axum::{
    extract::{Form, OriginalUri, Path, Query, Request, State},
    http::{StatusCode, Uri},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Extension,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tera::Context;
use tower_sessions::Session;

use crate::{
    appservices, commands,
    matrix::{self, server_name_from_mxid},
    rooms, users, Ctx,
};

const SESS_KEY: &str = "sess";
const FLASH_KEY: &str = "flash";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Flash {
    kind: String,
    text: String,
    #[serde(default)]
    log: Option<matrix::LogEntry>,
}

async fn take_flash(session: &Session) -> Option<Flash> {
    let f: Option<Flash> = session.get(FLASH_KEY).await.ok().flatten();
    if f.is_some() {
        let _ = session.remove::<Flash>(FLASH_KEY).await;
    }
    f
}

async fn set_flash(session: &Session, kind: &str, text: impl Into<String>) {
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

fn insert_flash(ctx: &mut Context, flash: Option<Flash>) {
    if let Some(f) = flash {
        ctx.insert("flash", &f);
    }
}

#[derive(serde::Serialize)]
struct CmdView {
    module: &'static str,
    action: &'static str,
    label: &'static str,
    desc: &'static str,
    danger: bool,
    fields: Vec<FieldView>,
}

#[derive(serde::Serialize)]
struct FieldView {
    name: &'static str,
    label: &'static str,
    kind: &'static str,
    placeholder: &'static str,
    required: bool,
}

impl From<commands::Cmd> for CmdView {
    fn from(c: commands::Cmd) -> Self {
        CmdView {
            module: c.module,
            action: c.action,
            label: c.label,
            desc: c.desc,
            danger: c.danger,
            fields: c
                .fields
                .iter()
                .map(|f| FieldView {
                    name: f.name,
                    label: f.label,
                    kind: match f.kind {
                        commands::FieldKind::Text => "text",
                        commands::FieldKind::Password => "password",
                        commands::FieldKind::Textarea => "textarea",
                        commands::FieldKind::Checkbox => "checkbox",
                        commands::FieldKind::Number => "number",
                    },
                    placeholder: f.placeholder,
                    required: f.required,
                })
                .collect(),
        }
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
    let login = st.matrix.login(f.username.trim(), &f.password).await?;

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
    render(&st, "dashboard.html", &ctx)
}

// Render a module page.
pub async fn module_page(
    State(st): State<Arc<Ctx>>,
    Extension(sess): Extension<matrix::Session>,
    Path(module): Path<String>,
) -> Response {
    let cmds = commands::by_module(&module);
    if cmds.is_empty() {
        return (StatusCode::NOT_FOUND, "unknown module").into_response();
    }
    let title = commands::MODULES
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, t)| *t)
        .unwrap_or(&module);

    let cmd_views: Vec<CmdView> = cmds.iter().map(|c| CmdView::from(**c)).collect();

    let mut ctx = base_ctx(&st, &sess, &module);
    ctx.insert("module", &module);
    ctx.insert("module_title", title);
    ctx.insert("cmds", &cmd_views);
    render(&st, "module.html", &ctx)
}

// Run a command and render the module page with the result.
pub async fn run_command(
    State(st): State<Arc<Ctx>>,
    Extension(sess): Extension<matrix::Session>,
    Path((module, action)): Path<(String, String)>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(cmd) = commands::find(&module, &action) else {
        return (StatusCode::NOT_FOUND, "unknown command").into_response();
    };

    let cmd_str = commands::render_template(cmd, &form);
    let (reply, error) = match st.matrix.run_admin(&sess, &cmd_str).await {
        Ok(r) => (Some(r), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    };

    let title = commands::MODULES
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, t)| *t)
        .unwrap_or(&module);
    let cmd_views: Vec<CmdView> = commands::by_module(&module)
        .into_iter()
        .map(|c| CmdView::from(*c))
        .collect();

    let mut ctx = base_ctx(&st, &sess, &module);
    ctx.insert("module", &module);
    ctx.insert("module_title", title);
    ctx.insert("cmds", &cmd_views);
    ctx.insert("ran_action", &action);
    ctx.insert("ran_cmd", &cmd_str);
    ctx.insert("form", &form);

    if let Some(r) = reply {
        ctx.insert(
            "reply_html",
            &if r.is_html {
                r.body.clone()
            } else {
                markdown_to_html(&r.body)
            },
        );
        ctx.insert("reply_raw", &r.body);
        ctx.insert("reply_sender", &r.sender);
    }
    if let Some(e) = error {
        ctx.insert("error", &e);
    }

    render(&st, "module.html", &ctx)
}

// Wrap the context with common fields.
fn base_ctx(st: &Ctx, sess: &matrix::Session, active: &str) -> Context {
    let mut ctx = Context::new();
    ctx.insert("user_id", &sess.user_id);
    ctx.insert("homeserver", st.matrix.homeserver());
    ctx.insert("admin_room_id", &sess.admin_room_id);
    ctx.insert("modules", commands::MODULES);
    ctx.insert("active", active);
    ctx
}

// Render a template and handle errors.
fn render(st: &Ctx, template: &str, ctx: &Context) -> Response {
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

// Users.

#[derive(Deserialize)]
pub struct CreateUserForm {
    pub username: String,
    pub password: String,
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

pub async fn users_list(
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

pub async fn users_create(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<CreateUserForm>,
) -> Response {
    let username = f.username.trim();
    if username.is_empty() || f.password.is_empty() {
        set_flash(&session, "error", "Username and password are required.").await;
        return Redirect::to("/users").into_response();
    }
    let cmd = format!("users create-user {username} {}", f.password);
    run_and_flash(
        &st,
        &sess,
        &session,
        &cmd,
        &format!("Created user {username}"),
    )
    .await;
    Redirect::to("/users").into_response()
}

pub async fn users_detail(
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
            ctx.insert("detail", &d);
            ctx.insert("joined_rooms_html", &raw_html);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "users/detail.html", &ctx)
}

pub async fn users_reset_password(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<PasswordForm>,
) -> Response {
    if f.password.is_empty() {
        set_flash(&session, "error", "Password is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
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
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn users_deactivate(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
) -> Response {
    let cmd = format!("users deactivate {mxid}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Deactivated {mxid}")).await;
    Redirect::to("/users").into_response()
}

pub async fn users_make_admin(
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
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn users_force_join(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<RoomForm>,
) -> Response {
    let room = f.room.trim();
    if room.is_empty() {
        set_flash(&session, "error", "Room is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
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
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn users_force_leave(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<RoomIdForm>,
) -> Response {
    let room = f.room_id.trim();
    if room.is_empty() {
        set_flash(&session, "error", "Room ID is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
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
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

pub async fn users_redact_event(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(mxid): Path<String>,
    Form(f): Form<EventIdForm>,
) -> Response {
    let evt = f.event_id.trim();
    if evt.is_empty() {
        set_flash(&session, "error", "Event ID is required.").await;
        return Redirect::to(&format!("/users/{mxid}")).into_response();
    }
    let cmd = format!("users redact-event {evt}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Redacted {evt}")).await;
    Redirect::to(&format!("/users/{mxid}")).into_response()
}

// ---- Rooms module ----

pub async fn rooms_list(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
) -> Response {
    let flash = take_flash(&session).await;

    let mut ctx = base_ctx(&st, &sess, "rooms");
    match rooms::list(&st.matrix, &sess).await {
        Ok((rows, log)) => {
            ctx.insert("rooms", &rows);
            install_log(&mut ctx, flash.as_ref(), log);
        }
        Err(e) => ctx.insert("error", &format!("{e:#}")),
    }
    insert_flash(&mut ctx, flash);
    render(&st, "rooms/list.html", &ctx)
}

pub async fn rooms_detail(
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

// Merge the flash's attached log entry (if any) with this page's log, insert
// into ctx, and raise a `log_warning` flag if any entry looks like an error.
fn install_log(ctx: &mut Context, flash: Option<&Flash>, mut page_log: Vec<matrix::LogEntry>) {
    let merged = if let Some(entry) = flash.and_then(|f| f.log.clone()) {
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
}

pub async fn rooms_ban(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms moderation ban-room {room_id}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Banned {room_id}")).await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn rooms_unban(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(room_id): Path<String>,
) -> Response {
    let cmd = format!("rooms moderation unban-room {room_id}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Unbanned {room_id}")).await;
    Redirect::to(&format!("/rooms/{room_id}")).into_response()
}

pub async fn rooms_federation_enable(
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

pub async fn rooms_federation_disable(
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

#[derive(Deserialize)]
pub struct DeleteRoomForm {
    #[serde(default)]
    pub force: Option<String>,
}

pub async fn rooms_delete(
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

// Run an admin command and set a one-shot flash message based on the outcome.
// Tuwunel's bot replies are free-form; treat any reply body starting with "error" as an error.
async fn run_and_flash(
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

// Appservices module.
#[derive(Deserialize)]
pub struct RegisterAppserviceForm {
    pub yaml: String,
}

pub async fn appservices_list(
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

pub async fn appservices_detail(
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

pub async fn appservices_register(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Form(f): Form<RegisterAppserviceForm>,
) -> Response {
    let yaml = f.yaml.trim();
    if yaml.is_empty() {
        set_flash(&session, "error", "Registration YAML is required.").await;
        return Redirect::to("/appservices").into_response();
    }
    let cmd = format!("appservices register\n```\n{yaml}\n```");
    run_and_flash(&st, &sess, &session, &cmd, "Registered appservice").await;
    Redirect::to("/appservices").into_response()
}

pub async fn appservices_unregister(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Extension(sess): Extension<matrix::Session>,
    Path(id): Path<String>,
) -> Response {
    let cmd = format!("appservices unregister {id}");
    run_and_flash(&st, &sess, &session, &cmd, &format!("Unregistered {id}")).await;
    Redirect::to("/appservices").into_response()
}

// Convert markdown to HTML for rendering command replies.
fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::with_capacity(md.len() * 2);
    html::push_html(&mut out, parser);
    out
}
