//! Domain logic for the Media module.
//!
//! Tuwunel does not expose a listable media index over the admin-bot,
//! so this module is tools-oriented: a lookup by MXC URL and a set of
//! targeted purge commands.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub mxc: String,
    pub fields: Vec<(String, String)>,
    pub raw: String,
}

pub async fn file_info(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    mxc: &str,
) -> Result<(FileInfo, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let cmd = format!("media get-file-info {mxc}");
    let reply = mx.run_admin(sess, &cmd).await?;
    log.push(matrix::LogEntry {
        cmd,
        body: reply.body.clone(),
        is_error: matrix::is_error_reply(&reply.body),
    });
    let fields = parse::media_file_info(&reply.body).unwrap_or_default();
    Ok((
        FileInfo {
            mxc: mxc.to_string(),
            fields,
            raw: reply.body,
        },
        log,
    ))
}
