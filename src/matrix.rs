use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use tracing::{debug, info};
use uuid::Uuid;

use crate::parse;

// How long to wait for the admin bot's reply before giving up.
const REPLY_DEADLINE: Duration = Duration::from_secs(10);

// Per-iteration /sync timeout.
const SYNC_LONGPOLL_MS: u64 = 15_000;

// Fallback when config leaves `device_id` / `device_display_name` blank.
const DEFAULT_DEVICE_LABEL: &str = "tuwunel-admin";

/// Trim whitespace and trailing slashes from a homeserver URL.
pub fn normalize(hs: &str) -> String {
    hs.trim().trim_end_matches('/').to_string()
}

/// Minimal Matrix Client-Server API wrapper.
#[derive(Clone)]
pub struct Matrix {
    http: Client,
}

impl Default for Matrix {
    fn default() -> Self {
        Self {
            http: Client::builder()
                .pool_idle_timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
        }
    }
}

impl Matrix {
    pub fn new() -> Self {
        Self::default()
    }

    /// /sync filter scoped to one room, timeline-only.
    fn room_filter(room_id: &str) -> String {
        json!({
            "room": {
                "rooms": [room_id],
                "timeline": { "limit": 20 },
                "state": { "types": [], "lazy_load_members": true },
                "ephemeral": { "types": [] },
                "account_data": { "types": [] },
            },
            "presence": { "types": [] },
            "account_data": { "types": [] },
        })
        .to_string()
    }

    /// POST /_matrix/client/v3/login with m.login.password.
    pub async fn login(
        &self,
        homeserver: &str,
        user: &str,
        password: &str,
        device_id: &str,
        device_display_name: &str,
    ) -> Result<LoginResult> {
        fn or_fallback(s: &str) -> &str {
            if s.is_empty() {
                DEFAULT_DEVICE_LABEL
            } else {
                s
            }
        }
        let body = json!({
            "type": "m.login.password",
            "identifier": { "type": "m.id.user", "user": user },
            "password": password,
            "device_id": or_fallback(device_id),
            "initial_device_display_name": or_fallback(device_display_name),
        });
        let res: Value = self
            .http
            .post(format!("{homeserver}/_matrix/client/v3/login"))
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .context("login failed: check username and password")?
            .json()
            .await?;

        Ok(LoginResult {
            user_id: res["user_id"]
                .as_str()
                .ok_or_else(|| anyhow!("no user_id"))?
                .to_string(),
            access_token: res["access_token"]
                .as_str()
                .ok_or_else(|| anyhow!("no access_token"))?
                .to_string(),
            device_id: res["device_id"].as_str().unwrap_or_default().to_string(),
        })
    }

    pub async fn logout(&self, homeserver: &str, token: &str) -> Result<()> {
        self.http
            .post(format!("{homeserver}/_matrix/client/v3/logout"))
            .bearer_auth(token)
            .send()
            .await?;
        Ok(())
    }

    pub async fn resolve_alias(
        &self,
        homeserver: &str,
        token: &str,
        alias: &str,
    ) -> Result<String> {
        let alias_enc = urlencoding::encode(alias);
        let res: Value = self
            .http
            .get(format!(
                "{homeserver}/_matrix/client/v3/directory/room/{alias_enc}"
            ))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("resolving alias {alias}"))?
            .json()
            .await?;
        Ok(res["room_id"]
            .as_str()
            .ok_or_else(|| anyhow!("no room_id in alias response"))?
            .to_string())
    }

    /// Members currently joined to `room_id`.
    pub async fn joined_members(
        &self,
        homeserver: &str,
        token: &str,
        room_id: &str,
    ) -> Result<HashSet<String>> {
        let rid = urlencoding::encode(room_id);
        let res: Value = self
            .http
            .get(format!(
                "{homeserver}/_matrix/client/v3/rooms/{rid}/joined_members"
            ))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(res["joined"]
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default())
    }

    /// Returns the event_id of the sent message.
    pub async fn send_text(
        &self,
        homeserver: &str,
        token: &str,
        room_id: &str,
        body: &str,
    ) -> Result<String> {
        let txn = Uuid::new_v4();
        let rid = urlencoding::encode(room_id);
        let res: Value = self
            .http
            .put(format!(
                "{homeserver}/_matrix/client/v3/rooms/{rid}/send/m.room.message/{txn}"
            ))
            .bearer_auth(token)
            .json(&json!({ "msgtype": "m.text", "body": body }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(res["event_id"]
            .as_str()
            .ok_or_else(|| anyhow!("no event_id"))?
            .to_string())
    }

    /// /sync narrowed to one room. Returns (next_batch, timeline_events).
    async fn sync_room(
        &self,
        homeserver: &str,
        token: &str,
        room_id: &str,
        since: Option<&str>,
        timeout_ms: u64,
    ) -> Result<(String, Vec<Value>)> {
        let filter = Self::room_filter(room_id);
        let timeout_str = timeout_ms.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("filter", filter.as_str()),
            ("timeout", timeout_str.as_str()),
        ];
        if let Some(s) = since {
            params.push(("since", s));
        }
        let res: Value = self
            .http
            .get(format!("{homeserver}/_matrix/client/v3/sync"))
            .query(&params)
            .bearer_auth(token)
            .timeout(Duration::from_millis(timeout_ms + 10_000))
            .send()
            .await
            .context("sync request failed")?
            .error_for_status()
            .context("sync returned error status")?
            .json()
            .await
            .context("sync body was not JSON")?;

        let next = res["next_batch"].as_str().unwrap_or("").to_string();
        let events = res["rooms"]["join"][room_id]["timeline"]["events"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok((next, events))
    }

    /// Run an admin command and push its reply into `log` as a LogEntry.
    pub async fn run_logged(
        &self,
        sess: &Session,
        cmd: &str,
        log: &mut Vec<LogEntry>,
    ) -> Result<BotReply> {
        let reply = self.run_admin(sess, cmd).await?;
        log.push(LogEntry {
            cmd: cmd.to_string(),
            body: reply.body.clone(),
            is_error: is_error_reply(&reply.body),
        });
        Ok(reply)
    }

    /// Post `cmd` into the admin room and long-poll /sync for the bot's reply.
    pub async fn run_admin(&self, sess: &Session, cmd: &str) -> Result<BotReply> {
        // Tuwunel requires the "!admin " prefix.
        let wire = if cmd.starts_with("!admin ") {
            cmd.to_string()
        } else {
            format!("!admin {cmd}")
        };
        let cmd = wire.as_str();
        let (hs, tok, room) = (&sess.homeserver, &sess.access_token, &sess.admin_room_id);
        info!(room = %room, cmd = %cmd, "sending admin command");

        // Snapshot /sync so we only see events after our message.
        let (since, _) = self
            .sync_room(hs, tok, room, None, 0)
            .await
            .context("initial sync snapshot")?;
        debug!(since, "got sync snapshot token");

        let our_event = self
            .send_text(hs, tok, room, cmd)
            .await
            .context("posting command to admin room")?;
        info!(event_id = %our_event, "command posted");

        // Long-poll until we see a non-self m.room.message in the room.
        let deadline = std::time::Instant::now() + REPLY_DEADLINE;
        let mut cursor = since;
        loop {
            if std::time::Instant::now() >= deadline {
                bail!(
                    "no reply from admin bot in {}s. Verify the admin bot is joined to {} and that `{}` is a valid command.",
                    REPLY_DEADLINE.as_secs(),
                    room,
                    cmd
                );
            }

            let (next, events) = self
                .sync_room(hs, tok, room, Some(&cursor), SYNC_LONGPOLL_MS)
                .await
                .context("long-polling sync")?;
            cursor = next;

            debug!(count = events.len(), "sync returned events");
            for evt in &events {
                let ty = evt["type"].as_str().unwrap_or("");
                let sender = evt["sender"].as_str().unwrap_or("");
                debug!(ty, sender, "event");
                if ty != "m.room.message" || sender == sess.user_id {
                    continue;
                }
                // `body` is markdown-ish and parseable; `formatted_body` is pre-rendered HTML.
                let body = evt["content"]["body"].as_str().unwrap_or("").to_string();
                return Ok(BotReply {
                    sender: sender.to_string(),
                    body,
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub user_id: String,
    pub access_token: String,
    pub device_id: String,
}

/// Per-user session, persisted by `tower-sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub access_token: String,
    pub admin_room_id: String,
    pub homeserver: String,
}

// One admin command round-trip. `LogEntry` is `Deserialize` because it
// round-trips through the session store (flash messages); `BotReply` is a
// per-request value that never leaves process memory, so it isn't.
#[derive(Debug, Clone, Serialize)]
pub struct BotReply {
    pub sender: String,
    pub body: String,
}

/// One admin command round-trip, shown in the page's console log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub cmd: String,
    pub body: String,
    #[serde(default)]
    pub is_error: bool,
}

/// Detect failure replies from the admin bot.
///
/// Tuwunel wraps many benign responses (e.g. `"No rooms are banned."`) with
/// a `"Command failed with error:"` prefix and a fenced block, so the prefix
/// alone isn't reliable. We peek inside the fence and treat empty or
/// `"No ..."` payloads as success.
pub fn is_error_reply(body: &str) -> bool {
    let trimmed = body.trim();
    if parse::starts_with_ci(trimmed, "command failed with error:") {
        let inner = parse::fenced(trimmed).unwrap_or("").trim();
        return !(inner.is_empty() || parse::starts_with_ci(inner, "no "));
    }
    parse::starts_with_ci(trimmed, "error") || trimmed.contains("unrecognized subcommand")
}
