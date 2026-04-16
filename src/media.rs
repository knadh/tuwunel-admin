//! Media: lookup by MXC URL and targeted purge commands. Tuwunel does not
//! expose a listable media index over the admin bot, so there's no list view.

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
    let reply = mx
        .run_logged(sess, &format!("media get-file-info {mxc}"), &mut log)
        .await?;
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
