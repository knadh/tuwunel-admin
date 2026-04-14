//! Domain logic for the Users module.
//!
//! Builds typed rows from tuwunel's admin-room replies and exposes a flat API
//! that handlers can call. All command strings come from `commands`; this
//! module is the only place that composes multiple commands into one view.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

use crate::{matrix, parse};

#[derive(Debug, Clone, Serialize)]
pub struct UserRow {
    pub mxid: String,
    pub localpart: String,
    pub is_admin: bool,
    pub last_active: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserDetail {
    pub row: UserRow,
    /// Parsed joined rooms, if the bot reply was recognizable.
    pub joined_rooms: Option<Vec<parse::JoinedRoom>>,
    /// Raw reply body, always present for fallback display and debugging.
    pub joined_rooms_raw: String,
    /// Devices (from `query users list-devices-metadata`), if parseable.
    pub devices: Vec<parse::DeviceRow>,
    pub log: Vec<matrix::LogEntry>,
}

fn localpart(mxid: &str) -> String {
    mxid.strip_prefix('@')
        .and_then(|s| s.split_once(':').map(|(l, _)| l))
        .unwrap_or(mxid)
        .to_string()
}

pub async fn list(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<Vec<UserRow>> {
    let users_reply = mx.run_admin(sess, "users list-users").await?;
    let mxids = parse::list_users(&users_reply.body).unwrap_or_default();

    let admins: std::collections::HashSet<String> = mx
        .joined_members(&sess.access_token, &sess.admin_room_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut last_active_by_lp: HashMap<String, String> = HashMap::new();
    if let Ok(la) = mx.run_admin(sess, "users last-active").await {
        if let Some(rows) = parse::last_active(&la.body) {
            last_active_by_lp.extend(rows);
        }
    }

    let mut rows: Vec<UserRow> = mxids
        .into_iter()
        .map(|mxid| {
            let lp = localpart(&mxid);
            UserRow {
                last_active: last_active_by_lp.get(&lp).cloned(),
                localpart: lp,
                is_admin: admins.contains(&mxid),
                mxid,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.mxid.cmp(&b.mxid));
    Ok(rows)
}

pub async fn detail(mx: &matrix::Matrix, sess: &matrix::Session, mxid: &str) -> Result<UserDetail> {
    let rows = list(mx, sess).await?;
    let row = rows
        .into_iter()
        .find(|r| r.mxid == mxid)
        .unwrap_or(UserRow {
            localpart: localpart(mxid),
            mxid: mxid.to_string(),
            is_admin: false,
            last_active: None,
        });

    let mut log = Vec::new();

    let cmd = format!("users list-joined-rooms {mxid}");
    let reply = mx.run_admin(sess, &cmd).await?;
    log.push(matrix::LogEntry {
        cmd,
        body: reply.body.clone(),
        is_error: matrix::is_error_reply(&reply.body),
    });
    let joined_rooms = parse::list_joined_rooms(&reply.body);

    let devices = match mx
        .run_admin(sess, &format!("query users list-devices-metadata {mxid}"))
        .await
    {
        Ok(r) => {
            let parsed = parse::list_devices(&r.body).unwrap_or_default();
            log.push(matrix::LogEntry {
                cmd: format!("query users list-devices-metadata {mxid}"),
                body: r.body,
                is_error: false,
            });
            parsed
        }
        Err(_) => Vec::new(),
    };

    Ok(UserDetail {
        row,
        joined_rooms,
        joined_rooms_raw: reply.body,
        devices,
        log,
    })
}
