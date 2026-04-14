//! Domain logic for the Federation module.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub incoming: Vec<String>,
    pub incoming_raw: String,
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
    let reply = run(mx, sess, "federation incoming-federation", &mut log).await?;
    let incoming = parse::list_federated_rooms(&reply.body).unwrap_or_default();
    Ok(Overview {
        incoming,
        incoming_raw: reply.body,
        log,
    })
}
