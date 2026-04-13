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
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomDetail {
    pub row: RoomRow,
    /// Parsed member mxids, if the bot reply was recognizable.
    pub members: Option<Vec<String>>,
    /// Raw reply body, always present for fallback display and debugging.
    pub members_raw: String,
}

/// List all rooms the server knows about. `rooms list` in tuwunel is
/// paginated; we pull the first page. If the reply shape isn't recognised
/// the caller sees an empty list and the raw body in `list_raw`.
pub async fn list(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<(Vec<RoomRow>, String)> {
    let reply = mx.run_admin(sess, "rooms list 1").await?;
    let banned: HashSet<String> = match mx
        .run_admin(sess, "rooms moderation list-banned-rooms")
        .await
    {
        Ok(r) => parse::list_banned_rooms(&r.body)
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
            room_id: r.room_id,
            name: r.name,
            members: r.members,
        })
        .collect();

    Ok((rows, reply.body))
}

pub async fn detail(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    room_id: &str,
) -> Result<RoomDetail> {
    let (rows, _) = list(mx, sess).await?;
    let row = rows
        .into_iter()
        .find(|r| r.room_id == room_id)
        .unwrap_or(RoomRow {
            room_id: room_id.to_string(),
            name: String::new(),
            members: 0,
            banned: false,
        });

    let reply = mx
        .run_admin(sess, &format!("rooms info list-joined-members {room_id}"))
        .await?;
    let members = parse::list_joined_members(&reply.body);
    Ok(RoomDetail {
        row,
        members,
        members_raw: reply.body,
    })
}
