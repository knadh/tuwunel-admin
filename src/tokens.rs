//! Domain logic for the Registration tokens module.
//!
//! Wraps tuwunel's `token` admin commands into typed rows. Mirrors the
//! pattern in `users.rs` / `rooms.rs`.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct TokenRow {
    pub token: String,
    pub uses_allowed: Option<u32>,
    pub pending: u32,
    pub completed: u32,
    pub expiration: Option<String>,
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
) -> Result<(Vec<TokenRow>, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let reply = run(mx, sess, "token list", &mut log).await?;
    let rows = parse::list_tokens(&reply.body)
        .unwrap_or_default()
        .into_iter()
        .map(|t| TokenRow {
            token: t.token,
            uses_allowed: t.uses_allowed,
            pending: t.pending,
            completed: t.completed,
            expiration: t.expiration,
        })
        .collect();
    Ok((rows, log))
}
