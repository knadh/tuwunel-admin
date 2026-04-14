//! Domain logic for the Server module.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

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

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub rows: Vec<(String, String)>,
    pub log: Vec<matrix::LogEntry>,
}

pub async fn config(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<Config> {
    let mut log = Vec::new();
    let reply = run(mx, sess, "server show-config", &mut log).await?;
    let rows = parse::config_table(&reply.body);
    Ok(Config { rows, log })
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub uptime: String,
    pub memory_sections: Vec<(String, Vec<(String, String)>)>,
    pub memory_raw: String,
    pub features: Vec<(String, bool)>,
    pub log: Vec<matrix::LogEntry>,
}

pub async fn stats(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<Stats> {
    let mut log = Vec::new();
    let uptime = run(mx, sess, "server uptime", &mut log)
        .await
        .map(|r| r.body.trim().trim_end_matches('.').trim().to_string())
        .unwrap_or_default();
    let mem = run(mx, sess, "server memory-usage", &mut log)
        .await
        .map(|r| r.body)
        .unwrap_or_default();
    let memory_sections = parse::memory_sections(&mem);
    let features = run(mx, sess, "server list-features", &mut log)
        .await
        .ok()
        .and_then(|r| parse::list_features(&r.body))
        .unwrap_or_default();
    Ok(Stats {
        uptime,
        memory_sections,
        memory_raw: mem,
        features,
        log,
    })
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Dashboard {
    pub uptime: String,
    pub db_mem_buffers: Option<String>,
    pub db_row_cache: Option<String>,
    pub db_table_readers: Option<String>,
    pub db_pending_write: Option<String>,
    pub user_count: usize,
    pub admin_count: usize,
    pub room_count: usize,
    pub room_count_capped: bool,
    pub local_rooms: usize,
    pub federated_rooms: usize,
    pub banned_rooms: usize,
    pub published_rooms: usize,
    pub appservices_count: usize,
    pub features_enabled: usize,
    pub features_total: usize,
}

/// Iterate `rooms list <page>` until a page is empty or errors. Capped at
/// `MAX_PAGES` (100 rooms/page) to bound latency on large deployments.
async fn count_rooms(mx: &matrix::Matrix, sess: &matrix::Session) -> (usize, bool) {
    const MAX_PAGES: u32 = 20;
    let mut total = 0usize;
    let mut capped = false;
    for page in 1..=MAX_PAGES {
        let cmd = format!("rooms list {page}");
        match mx.run_admin(sess, &cmd).await {
            Ok(r) => {
                let n = r
                    .body
                    .lines()
                    .filter(|l| l.trim_start().starts_with('!'))
                    .count();
                if n == 0 {
                    return (total, false);
                }
                total += n;
                if page == MAX_PAGES {
                    capped = true;
                }
            }
            Err(_) => return (total, false),
        }
    }
    (total, capped)
}

pub async fn dashboard(mx: &matrix::Matrix, sess: &matrix::Session) -> Dashboard {
    let mut d = Dashboard::default();

    if let Ok(r) = mx.run_admin(sess, "server uptime").await {
        d.uptime = r.body.trim().trim_end_matches('.').trim().to_string();
    }

    if let Ok(r) = mx.run_admin(sess, "server memory-usage").await {
        let db = parse::memory_database_section(&r.body);
        d.db_mem_buffers = db.get("Memory buffers").cloned();
        d.db_row_cache = db.get("Row cache").cloned();
        d.db_table_readers = db.get("Table readers").cloned();
        d.db_pending_write = db.get("Pending write").cloned();
    }

    if let Ok(r) = mx.run_admin(sess, "users list-users").await {
        d.user_count = parse::count_users(&r.body);
    }

    d.admin_count = mx
        .joined_members(&sess.access_token, &sess.admin_room_id)
        .await
        .map(|v| v.len())
        .unwrap_or(0);

    let (rooms, capped) = count_rooms(mx, sess).await;
    d.room_count = rooms;
    d.room_count_capped = capped;

    if let Ok(r) = mx.run_admin(sess, "federation incoming-federation").await {
        d.federated_rooms = parse::list_federated_rooms(&r.body)
            .unwrap_or_default()
            .len();
    }
    d.local_rooms = d.room_count.saturating_sub(d.federated_rooms);
    if let Ok(r) = mx
        .run_admin(sess, "rooms moderation list-banned-rooms")
        .await
    {
        d.banned_rooms = parse::list_banned_rooms(&r.body).unwrap_or_default().len();
    }
    if let Ok(r) = mx.run_admin(sess, "rooms directory list 1").await {
        d.published_rooms = parse::list_published_rooms(&r.body)
            .unwrap_or_default()
            .len();
    }
    if let Ok(r) = mx.run_admin(sess, "appservices list").await {
        d.appservices_count = parse::list_appservices(&r.body).unwrap_or_default().len();
    }
    if let Ok(r) = mx.run_admin(sess, "server list-features").await {
        let f = parse::list_features(&r.body).unwrap_or_default();
        d.features_total = f.len();
        d.features_enabled = f.iter().filter(|(_, on)| *on).count();
    }

    d
}

