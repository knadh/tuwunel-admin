//! Server: `server` admin commands composed into typed rows.

use anyhow::Result;
use serde::Serialize;

use crate::{matrix, parse};

/// Strip the trailing period that tuwunel's `server uptime` reply carries.
fn clean_uptime(body: &str) -> String {
    body.trim().trim_end_matches('.').trim().to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub rows: Vec<(String, String)>,
    pub log: Vec<matrix::LogEntry>,
}

pub async fn config(mx: &matrix::Matrix, sess: &matrix::Session) -> Result<Config> {
    let mut log = Vec::new();
    let reply = mx.run_logged(sess, "server show-config", &mut log).await?;
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
    let uptime = mx
        .run_logged(sess, "server uptime", &mut log)
        .await
        .map(|r| clean_uptime(&r.body))
        .unwrap_or_default();
    let mem = mx
        .run_logged(sess, "server memory-usage", &mut log)
        .await
        .map(|r| r.body)
        .unwrap_or_default();
    let memory_sections = parse::memory_sections(&mem);
    let features = mx
        .run_logged(sess, "server list-features", &mut log)
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

/// Count rooms by paging through `rooms list <n>`. Bounded by `MAX_PAGES`
/// (≈ MAX_PAGES × 100 rooms) to cap latency on large deployments; the second
/// return value is true when we hit the cap.
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
    /// Pass the reply body to `f` on success; swallow errors.
    async fn on_ok<F>(mx: &matrix::Matrix, sess: &matrix::Session, cmd: &str, f: F)
    where
        F: FnOnce(&str),
    {
        if let Ok(r) = mx.run_admin(sess, cmd).await {
            f(&r.body);
        }
    }

    /// Length of the parsed list, or 0 on any failure.
    async fn count<T>(
        mx: &matrix::Matrix,
        sess: &matrix::Session,
        cmd: &str,
        parser: impl FnOnce(&str) -> Option<Vec<T>>,
    ) -> usize {
        mx.run_admin(sess, cmd)
            .await
            .ok()
            .and_then(|r| parser(&r.body))
            .map_or(0, |v| v.len())
    }

    let mut d = Dashboard::default();

    on_ok(mx, sess, "server uptime", |b| d.uptime = clean_uptime(b)).await;
    on_ok(mx, sess, "server memory-usage", |b| {
        let db = parse::memory_database_section(b);
        d.db_mem_buffers = db.get("Memory buffers").cloned();
        d.db_row_cache = db.get("Row cache").cloned();
        d.db_table_readers = db.get("Table readers").cloned();
        d.db_pending_write = db.get("Pending write").cloned();
    })
    .await;
    on_ok(mx, sess, "users list-users", |b| {
        d.user_count = parse::count_users(b)
    })
    .await;

    d.admin_count = mx
        .joined_members(&sess.homeserver, &sess.access_token, &sess.admin_room_id)
        .await
        .map_or(0, |v| v.len());

    (d.room_count, d.room_count_capped) = count_rooms(mx, sess).await;

    d.federated_rooms = count(
        mx,
        sess,
        "federation incoming-federation",
        parse::list_federated_rooms,
    )
    .await;
    d.local_rooms = d.room_count.saturating_sub(d.federated_rooms);
    d.banned_rooms = count(
        mx,
        sess,
        "rooms moderation list-banned-rooms",
        parse::list_banned_rooms,
    )
    .await;
    d.published_rooms = count(
        mx,
        sess,
        "rooms directory list 1",
        parse::list_published_rooms,
    )
    .await;
    d.appservices_count = count(mx, sess, "appservices list", parse::list_appservices).await;

    on_ok(mx, sess, "server list-features", |b| {
        let f = parse::list_features(b).unwrap_or_default();
        d.features_total = f.len();
        d.features_enabled = f.iter().filter(|(_, on)| *on).count();
    })
    .await;

    d
}
