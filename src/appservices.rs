//! Domain logic for the Appservices module.
//!
//! Wraps tuwunel's `appservices` admin commands into typed rows and a detail
//! view. Mirrors the pattern in `users.rs` / `rooms.rs`.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct AppserviceRow {
    pub id: String,
    /// Pulled from the registration YAML when we have it. Empty until the
    /// detail page has been visited or the config lookup succeeds inline.
    pub sender_localpart: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppserviceDetail {
    pub row: AppserviceRow,
    /// Parsed YAML fields extracted best-effort from the registration.
    pub fields: Vec<(String, String)>,
    /// Raw YAML body for fallback display.
    pub config_yaml: String,
    /// Raw bot reply body (includes the `Config for {id}:` header).
    pub config_raw: String,
    pub log: Vec<matrix::LogEntry>,
}

async fn run(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    cmd: &str,
    log: &mut Vec<matrix::LogEntry>,
) -> Result<matrix::BotReply> {
    let reply = mx.run_admin(sess, cmd).await?;
    log.push(matrix::LogEntry {
        cmd: cmd.to_string(),
        body: reply.body.clone(),
        is_error: matrix::is_error_reply(&reply.body),
    });
    Ok(reply)
}

pub async fn list(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
) -> Result<(Vec<AppserviceRow>, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let reply = run(mx, sess, "appservices list", &mut log).await?;
    let ids = parse::list_appservices(&reply.body).unwrap_or_default();
    let rows: Vec<AppserviceRow> = ids
        .into_iter()
        .map(|id| AppserviceRow {
            id,
            sender_localpart: String::new(),
            url: String::new(),
        })
        .collect();
    Ok((rows, log))
}

pub async fn detail(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    id: &str,
) -> Result<AppserviceDetail> {
    let (rows, mut log) = list(mx, sess).await?;
    let row = rows
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or(AppserviceRow {
            id: id.to_string(),
            sender_localpart: String::new(),
            url: String::new(),
        });

    let reply = run(mx, sess, &format!("appservices show-config {id}"), &mut log).await?;
    let config_yaml = parse::appservice_config_yaml(&reply.body).unwrap_or_default();
    let fields = extract_fields(&config_yaml);

    let row = AppserviceRow {
        id: row.id,
        sender_localpart: find_field(&fields, "sender_localpart"),
        url: find_field(&fields, "url"),
    };

    Ok(AppserviceDetail {
        row,
        fields,
        config_yaml,
        config_raw: reply.body,
        log,
    })
}

/// Best-effort flat YAML field extractor. Pulls top-level `key: value`
/// lines with scalar values (ignores nested maps/lists). Good enough for
/// surfacing headline fields on the detail page without pulling in a
/// YAML parser.
fn extract_fields(yaml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in yaml.lines() {
        if line.starts_with(' ') || line.starts_with('\t') || line.starts_with('-') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
        if k.is_empty() || v.is_empty() {
            continue;
        }
        out.push((k.to_string(), v));
    }
    out
}

fn find_field(fields: &[(String, String)], key: &str) -> String {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}
