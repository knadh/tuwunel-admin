use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tera::Context;
use tower_sessions::Session;

use crate::{
    commands,
    matrix::{self, server_name_from_mxid},
    Ctx,
};

const SESS_KEY: &str = "sess";

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

// Render the login page.
pub async fn login_page(State(st): State<Arc<Ctx>>) -> Response {
    render(&st, "login.html", &login_ctx(&st, None))
}

// Wrap the context.
fn login_ctx(st: &Ctx, error: Option<&str>) -> Context {
    let mut ctx = Context::new();
    ctx.insert("homeserver", st.matrix.homeserver());
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
    Form(f): Form<LoginForm>,
) -> Response {
    match do_login(&st, session, f).await {
        Ok(()) => Redirect::to("/").into_response(),
        Err(e) => render(&st, "login.html", &login_ctx(&st, Some(&format!("{e:#}")))),
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
            "user {} is not a member of the admin room {} — not a server admin",
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

// Get the current session, if any. Used by handlers to check auth and get user info.
async fn current_session(session: &Session) -> Option<matrix::Session> {
    session
        .get::<matrix::Session>(SESS_KEY)
        .await
        .ok()
        .flatten()
}

// Render the dashboard page.
pub async fn index(State(st): State<Arc<Ctx>>, session: Session) -> Response {
    let Some(sess) = current_session(&session).await else {
        return Redirect::to("/login").into_response();
    };
    let mut ctx = base_ctx(&st, &sess, "home");
    ctx.insert("modules", commands::MODULES);
    render(&st, "dashboard.html", &ctx)
}

// Render a module page.
pub async fn module_page(
    State(st): State<Arc<Ctx>>,
    session: Session,
    Path(module): Path<String>,
) -> Response {
    let Some(sess) = current_session(&session).await else {
        return Redirect::to("/login").into_response();
    };
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
    session: Session,
    Path((module, action)): Path<(String, String)>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(sess) = current_session(&session).await else {
        return Redirect::to("/login").into_response();
    };
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
