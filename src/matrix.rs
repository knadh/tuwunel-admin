use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info};
use uuid::Uuid;

// Bot command req-resp timeout.
const REPLY_DEADLINE: Duration = Duration::from_secs(10);

/// Max timeout for a single long-poll sync iteration in ms.
const SYNC_LONGPOLL_MS: u64 = 15_000;

/// A simple Matrix Client-Server API wrapper bound to a single homeserver.
#[derive(Clone)]
pub struct Matrix {
    http: Client,
    homeserver: String,
}

impl Matrix {
    pub fn new(homeserver: impl Into<String>) -> Self {
        Self {
            http: Client::builder()
                .pool_idle_timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
            homeserver: homeserver.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn homeserver(&self) -> &str {
        &self.homeserver
    }

    /// URL-encoded filter JSON scoped to one room, timeline-only.
    fn room_filter(room_id: &str) -> String {
        let filter = json!({
            "room": {
                "rooms": [room_id],
                "timeline": { "limit": 20 },
                "state": { "types": [], "lazy_load_members": true },
                "ephemeral": { "types": [] },
                "account_data": { "types": [] },
            },
            "presence": { "types": [] },
            "account_data": { "types": [] },
        });
        urlencoding::encode(&filter.to_string()).into_owned()
    }

    /// POST /_matrix/client/v3/login with m.login.password.
    pub async fn login(&self, user: &str, password: &str) -> Result<LoginResult> {
        let body = json!({
            "type": "m.login.password",
            "identifier": { "type": "m.id.user", "user": user },
            "password": password,
            "initial_device_display_name": "tuwunel-admin",
        });
        let res: Value = self
            .http
            .post(format!("{}/_matrix/client/v3/login", self.homeserver))
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .context("login failed — check username and password")?
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

    pub async fn logout(&self, token: &str) -> Result<()> {
        self.http
            .post(format!("{}/_matrix/client/v3/logout", self.homeserver))
            .bearer_auth(token)
            .send()
            .await?;
        Ok(())
    }

    /// Resolve a room alias to a room ID.
    pub async fn resolve_alias(&self, token: &str, alias: &str) -> Result<String> {
        let alias_enc = urlencoding::encode(alias);
        let res: Value = self
            .http
            .get(format!(
                "{}/_matrix/client/v3/directory/room/{alias_enc}",
                self.homeserver
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

    /// Fetch joined members of a room. Returns mxids.
    pub async fn joined_members(&self, token: &str, room_id: &str) -> Result<Vec<String>> {
        let rid = urlencoding::encode(room_id);
        let res: Value = self
            .http
            .get(format!(
                "{}/_matrix/client/v3/rooms/{rid}/joined_members",
                self.homeserver
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

    /// Send a message to a room. Returns event_id.
    pub async fn send_text(&self, token: &str, room_id: &str, body: &str) -> Result<String> {
        let txn = Uuid::new_v4();
        let rid = urlencoding::encode(room_id);
        let res: Value = self
            .http
            .put(format!(
                "{}/_matrix/client/v3/rooms/{rid}/send/m.room.message/{txn}",
                self.homeserver
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

    /// Narrow /sync scoped to one room, returning (next_batch, timeline_events).
    async fn sync_room(
        &self,
        token: &str,
        room_id: &str,
        since: Option<&str>,
        timeout_ms: u64,
    ) -> Result<(String, Vec<Value>)> {
        let filter = Self::room_filter(room_id);
        let mut url = format!(
            "{}/_matrix/client/v3/sync?filter={filter}&timeout={timeout_ms}",
            self.homeserver
        );
        if let Some(s) = since {
            url.push_str("&since=");
            url.push_str(&urlencoding::encode(s));
        }
        let req_timeout = Duration::from_millis(timeout_ms + 10_000);
        let res: Value = self
            .http
            .get(&url)
            .bearer_auth(token)
            .timeout(req_timeout)
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

    /// Run an admin command. Post to the admin room and wait for the bot reply.
    pub async fn run_admin(&self, sess: &Session, cmd: &str) -> Result<BotReply> {
        // Tuwunel admin commands must be prefixed with "!admin ".
        let wire = if cmd.starts_with("!admin ") {
            cmd.to_string()
        } else {
            format!("!admin {cmd}")
        };
        let cmd = wire.as_str();
        info!(room = %sess.admin_room_id, cmd = %cmd, "sending admin command");

        // Snapshot sync position so we only see events after our message.
        let (since, _) = self
            .sync_room(&sess.access_token, &sess.admin_room_id, None, 0)
            .await
            .context("initial sync (snapshot)")?;
        debug!(since, "got sync snapshot token");

        let our_event = self
            .send_text(&sess.access_token, &sess.admin_room_id, cmd)
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
                    sess.admin_room_id,
                    cmd
                );
            }

            let (next, events) = self
                .sync_room(
                    &sess.access_token,
                    &sess.admin_room_id,
                    Some(&cursor),
                    SYNC_LONGPOLL_MS,
                )
                .await
                .context("long-poll sync")?;
            cursor = next;

            debug!(count = events.len(), "sync returned events");
            for evt in &events {
                let ty = evt["type"].as_str().unwrap_or("");
                let sender = evt["sender"].as_str().unwrap_or("");
                debug!(ty, sender, "event");
                if ty != "m.room.message" || sender == sess.user_id {
                    continue;
                }
                let body = evt["content"]["formatted_body"]
                    .as_str()
                    .or_else(|| evt["content"]["body"].as_str())
                    .unwrap_or("")
                    .to_string();
                let format = evt["content"]["format"].as_str().unwrap_or("").to_string();
                return Ok(BotReply {
                    sender: sender.to_string(),
                    body,
                    is_html: format == "org.matrix.custom.html",
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

/// Per-user session persisted in the session store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub access_token: String,
    pub admin_room_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BotReply {
    pub sender: String,
    pub body: String,
    pub is_html: bool,
}

/// Derive server name from an mxid (`@user:server`).
pub fn server_name_from_mxid(mxid: &str) -> Option<&str> {
    mxid.split_once(':').map(|(_, s)| s)
}
