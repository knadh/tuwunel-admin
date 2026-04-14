//! Domain logic for the Server module.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub uptime: String,
    pub memory: String,
    pub features: Vec<(String, bool)>,
    pub backups_raw: String,
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

pub async fn overview(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<Overview> {
    let mut log = Vec::new();
    let uptime = run(mx, sess, "server uptime", &mut log)
        .await
        .map(|r| r.body.trim().to_string())
        .unwrap_or_default();
    let memory = run(mx, sess, "server memory-usage", &mut log)
        .await
        .map(|r| r.body)
        .unwrap_or_default();
    let features = run(mx, sess, "server list-features", &mut log)
        .await
        .ok()
        .and_then(|r| parse::list_features(&r.body))
        .unwrap_or_default();
    let backups_raw = run(mx, sess, "server list-backups", &mut log)
        .await
        .map(|r| r.body)
        .unwrap_or_default();
    let config_raw = run(mx, sess, "server show-config", &mut log)
        .await
        .map(|r| r.body)
        .unwrap_or_default();
    Ok(Overview {
        uptime,
        memory,
        features,
        backups_raw,
        config_raw,
        log,
    })
}
