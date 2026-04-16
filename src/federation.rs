//! Federation: `federation` admin commands composed into typed rows.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub incoming: Vec<String>,
    pub incoming_raw: String,
    pub log: Vec<matrix::LogEntry>,
}

pub async fn overview(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<Overview> {
    let mut log = Vec::new();
    let reply = mx
        .run_logged(sess, "federation incoming-federation", &mut log)
        .await?;
    let incoming = parse::list_federated_rooms(&reply.body).unwrap_or_default();
    Ok(Overview {
        incoming,
        incoming_raw: reply.body,
        log,
    })
}
