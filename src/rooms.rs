//! Rooms: admin-bot commands composed into typed rows for handlers.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;

use crate::{matrix, parse};

#[derive(Debug, Default, Clone, Serialize)]
pub struct RoomRow {
    pub room_id: String,
    pub name: String,
    pub members: u32,
    pub banned: bool,
    pub federated: bool,
    pub published: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomDetail {
    pub row: RoomRow,
    /// None if the bot reply couldn't be parsed.
    pub members: Option<Vec<String>>,
    /// Raw reply body, for markdown fallback rendering.
    pub members_raw: String,
    pub aliases: Vec<String>,
    pub topic: Option<String>,
    pub log: Vec<matrix::LogEntry>,
}

#[derive(Debug, Default, Clone)]
pub struct ListOpts {
    pub page: Option<u32>,
    pub exclude_banned: bool,
    pub exclude_disabled: bool,
}

/// Run `cmd`, parse the reply through `f`, or return `T::default()` on error.
async fn try_parse<T: Default>(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    cmd: &str,
    log: &mut Vec<matrix::LogEntry>,
    f: impl FnOnce(&str) -> Option<T>,
) -> T {
    mx.run_logged(sess, cmd, log)
        .await
        .ok()
        .and_then(|r| f(&r.body))
        .unwrap_or_default()
}

pub async fn list(
    mx: &matrix::Matrix,
    sess: &matrix::Session,
    opts: &ListOpts,
) -> Result<(Vec<RoomRow>, Vec<matrix::LogEntry>)> {
    let mut log = Vec::new();
    let mut cmd = String::from("rooms list");
    if opts.exclude_banned {
        cmd.push_str(" --exclude-banned");
    }
    if opts.exclude_disabled {
        cmd.push_str(" --exclude-disabled");
    }
    cmd.push(' ');
    cmd.push_str(&opts.page.unwrap_or(1).to_string());
    let reply = mx.run_logged(sess, &cmd, &mut log).await?;

    let set = |v: Vec<String>| -> HashSet<String> { v.into_iter().collect() };
    let banned = set(try_parse(
        mx,
        sess,
        "rooms moderation list-banned-rooms",
        &mut log,
        parse::list_banned_rooms,
    )
    .await);
    let federated = set(try_parse(
        mx,
        sess,
        "federation incoming-federation",
        &mut log,
        parse::list_federated_rooms,
    )
    .await);
    let published = set(try_parse(
        mx,
        sess,
        "rooms directory list 1",
        &mut log,
        parse::list_published_rooms,
    )
    .await);

    let rows: Vec<RoomRow> = parse::list_rooms(&reply.body)
        .unwrap_or_default()
        .into_iter()
        .map(|r| RoomRow {
            banned: banned.contains(&r.room_id),
            federated: federated.contains(&r.room_id),
            published: published.contains(&r.room_id),
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
    let (rows, mut log) = list(mx, sess, &ListOpts::default()).await?;
    let row = rows
        .into_iter()
        .find(|r| r.room_id == room_id)
        .unwrap_or(RoomRow {
            room_id: room_id.to_string(),
            ..Default::default()
        });

    let reply = mx
        .run_logged(
            sess,
            &format!("rooms info list-joined-members {room_id}"),
            &mut log,
        )
        .await?;
    let members = parse::list_joined_members(&reply.body);

    let aliases = try_parse(
        mx,
        sess,
        &format!("rooms alias list {room_id}"),
        &mut log,
        parse::aliases_for_room,
    )
    .await;
    let topic = mx
        .run_logged(
            sess,
            &format!("rooms info view-room-topic {room_id}"),
            &mut log,
        )
        .await
        .ok()
        .and_then(|r| parse::room_topic(&r.body));

    Ok(RoomDetail {
        row,
        members,
        members_raw: reply.body,
        aliases,
        topic,
        log,
    })
}
