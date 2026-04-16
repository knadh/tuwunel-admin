//! Appservices: `appservices` admin commands composed into typed rows.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Default, Clone, Serialize)]
pub struct AppserviceRow {
    pub id: String,
    /// Empty until the detail page has populated it from the registration YAML.
    pub sender_localpart: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppserviceDetail {
    pub row: AppserviceRow,
    pub fields: Vec<(String, String)>,
    pub config_yaml: String,
    /// Raw bot reply (includes the `Config for {id}:` header).
    pub config_raw: String,
    pub log: Vec<matrix::LogEntry>,
}

pub async fn list(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
) -> Result<(Vec<AppserviceRow>, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let reply = mx.run_logged(sess, "appservices list", &mut log).await?;
    let rows = parse::list_appservices(&reply.body)
        .unwrap_or_default()
        .into_iter()
        .map(|id| AppserviceRow {
            id,
            ..Default::default()
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
    let mut row = rows
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or(AppserviceRow {
            id: id.to_string(),
            ..Default::default()
        });

    let reply = mx
        .run_logged(sess, &format!("appservices show-config {id}"), &mut log)
        .await?;
    let config_yaml = parse::appservice_config_yaml(&reply.body).unwrap_or_default();
    let fields = extract_fields(&config_yaml);

    row.sender_localpart = find_field(&fields, "sender_localpart");
    row.url = find_field(&fields, "url");

    Ok(AppserviceDetail {
        row,
        fields,
        config_yaml,
        config_raw: reply.body,
        log,
    })
}

/// Top-level `key: value` scalars only. Ignores nested maps and lists, which
/// is enough for the headline fields shown on the detail page.
fn extract_fields(yaml: &str) -> Vec<(String, String)> {
    yaml.lines()
        .filter(|l| !l.starts_with([' ', '\t', '-']))
        .filter_map(|l| l.split_once(':'))
        .filter_map(|(k, v)| {
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'');
            (!k.is_empty() && !v.is_empty()).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn find_field(fields: &[(String, String)], key: &str) -> String {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}
