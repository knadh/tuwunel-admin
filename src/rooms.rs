//! Domain logic for the Rooms module.
//!
//! Builds typed rows from tuwunel's admin-room replies and exposes a flat API
//! that handlers can call. Mirror of `users.rs`; all command strings come from
//! the admin-bot wire protocol, not from `commands.rs`.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct RoomRow {
    pub room_id: String,
    pub name: String,
    pub members: u32,
    pub banned: bool,
    pub federated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomDetail {
    pub row: RoomRow,
    /// Parsed member mxids, if the bot reply was recognizable.
    pub members: Option<Vec<String>>,
    /// Raw members reply body, for markdown fallback rendering.
    pub members_raw: String,
    /// Ordered log of every admin command run to build this page.
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

/// List all rooms the server knows about. Returns rows and the command log.
pub async fn list(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
) -> Result<(Vec<RoomRow>, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let reply = run(mx, sess, "rooms list 1", &mut log).await?;

    let banned: HashSet<String> =
        match run(mx, sess, "rooms moderation list-banned-rooms", &mut log).await {
            Ok(r) => parse::list_banned_rooms(&r.body)
                .unwrap_or_default()
                .into_iter()
                .collect(),
            Err(_) => HashSet::new(),
        };
    let federated: HashSet<String> =
        match run(mx, sess, "federation incoming-federation", &mut log).await {
            Ok(r) => parse::list_federated_rooms(&r.body)
                .unwrap_or_default()
                .into_iter()
                .collect(),
            Err(_) => HashSet::new(),
        };

    let rows: Vec<RoomRow> = parse::list_rooms(&reply.body)
        .unwrap_or_default()
        .into_iter()
        .map(|r| RoomRow {
            banned: banned.contains(&r.room_id),
            federated: federated.contains(&r.room_id),
            room_id: r.room_id,
            name: r.name,
            members: r.members,
        })
        .collect();

    Ok((rows, log))
}

pub async fn detail(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    room_id: &str,
) -> Result<RoomDetail> {
    let (rows, mut log) = list(mx, sess).await?;
    let row = rows
        .into_iter()
        .find(|r| r.room_id == room_id)
        .unwrap_or(RoomRow {
            room_id: room_id.to_string(),
            name: String::new(),
            members: 0,
            banned: false,
            federated: false,
        });

    let reply = run(
        mx,
        sess,
        &format!("rooms info list-joined-members {room_id}"),
        &mut log,
    )
    .await?;
    let members = parse::list_joined_members(&reply.body);
    Ok(RoomDetail {
        row,
        members,
        members_raw: reply.body,
        log,
    })
}
