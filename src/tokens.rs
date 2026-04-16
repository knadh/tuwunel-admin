//! Registration tokens: `token` admin commands composed into typed rows.

use anyhow::Result;

use crate::{matrix, parse};

pub use parse::TokenRow;

pub async fn list(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
) -> Result<(Vec<TokenRow>, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let reply = mx.run_logged(sess, "token list", &mut log).await?;
    Ok((parse::list_tokens(&reply.body).unwrap_or_default(), log))
}
