//! Parsers for tuwunel admin-bot reply bodies.
//!
//! Parsers are best-effort. When a reply does not match the expected shape
//! they return `None` and the caller falls back to rendering the raw markdown.

/// `Found N local user account(s):\n```\n@mxid\n@mxid\n...\n````
pub fn list_users(body: &str) -> Option<Vec<String>> {
    if !body.trim_start().starts_with("Found ") {
        return None;
    }
    let mxids: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('@') && l.contains(':'))
        .map(|l| l.to_string())
        .collect();
    if mxids.is_empty() {
        return None;
    }
    Some(mxids)
}

/// Line shape: `YYYY-MM-DDTHH:MM:SS.mmm localpart`. Returns (localpart, timestamp).
pub fn last_active(body: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        let Some((ts, rest)) = line.split_once(' ') else {
            continue;
        };
        if ts.len() < 10 || ts.as_bytes()[4] != b'-' {
            continue;
        }
        let localpart = rest.trim();
        if localpart.is_empty() {
            continue;
        }
        out.push((localpart.to_string(), ts.to_string()));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JoinedRoom {
    pub room_id: String,
    pub members: u32,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomRow {
    pub room_id: String,
    pub name: String,
    pub members: u32,
}

/// Best-effort parser for `rooms list` output. Accepts lines whose first
/// tab-separated field starts with `!` and tries to extract `Members: N`
/// and `Name: X` from the remaining fields (in any order). Unknown fields
/// are ignored. Returns None if no room-shaped lines are found.
pub fn list_rooms(body: &str) -> Option<Vec<RoomRow>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if !line.starts_with('!') {
            continue;
        }
        let mut parts = line.split('\t');
        let room_id = parts.next()?.trim().to_string();
        let mut name = String::new();
        let mut members: u32 = 0;
        for p in parts {
            let p = p.trim();
            if let Some(rest) = p.strip_prefix("Members:") {
                members = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = p.strip_prefix("Name:") {
                name = rest.trim().to_string();
            }
        }
        let name = if name == room_id { String::new() } else { name };
        out.push(RoomRow {
            room_id,
            name,
            members,
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Best-effort parser for `rooms info list-joined-members`. Extracts the
/// first mxid-shaped token (`@localpart:server`) from each line, ignoring
/// trailing display-name suffixes like ` | Deepa`, markdown list markers,
/// or any surrounding HTML.
pub fn list_joined_members(body: &str) -> Option<Vec<String>> {
    let mut mxids: Vec<String> = Vec::new();
    for line in body.lines() {
        let Some(at) = line.find('@') else {
            continue;
        };
        let rest = &line[at..];
        let end = rest
            .find(|c: char| c == '|' || c == '<' || c.is_whitespace())
            .unwrap_or(rest.len());
        let token = rest[..end].trim();
        if token.len() > 2 && token.starts_with('@') && token.contains(':') {
            mxids.push(token.to_string());
        }
    }
    if mxids.is_empty() {
        None
    } else {
        Some(mxids)
    }
}

/// Best-effort parser for `federation incoming-federation`. Returns the
/// list of room_ids with incoming federation enabled.
pub fn list_federated_rooms(body: &str) -> Option<Vec<String>> {
    let ids: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('!'))
        .map(|l| {
            l.split(|c: char| c.is_whitespace() || c == '\t')
                .next()
                .unwrap_or(l)
                .trim()
                .to_string()
        })
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// Best-effort parser for `rooms moderation list-banned-rooms`. Returns
/// the list of banned room_ids.
pub fn list_banned_rooms(body: &str) -> Option<Vec<String>> {
    let ids: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('!'))
        .map(|l| l.split('\t').next().unwrap_or(l).trim().to_string())
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// `Appservices (N): id1, id2, id3`. Returns the list of IDs (possibly empty
/// for N=0). Returns None only if the header is missing.
pub fn list_appservices(body: &str) -> Option<Vec<String>> {
    let trimmed = body.trim();
    let after = trimmed.strip_prefix("Appservices")?;
    let colon = after.find(':')?;
    let list = after[colon + 1..].trim();
    if list.is_empty() {
        return Some(Vec::new());
    }
    let ids: Vec<String> = list
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Some(ids)
}

/// Extract the YAML payload from an `appservices show-config` reply. The reply
/// shape is `Config for {id}:\n\n```yaml\n...\n```` — we return the fenced
/// body. Falls back to None if no fenced block is found.
pub fn appservice_config_yaml(body: &str) -> Option<String> {
    let s = body;
    let open = s.find("```")?;
    let after_open = &s[open + 3..];
    let first_nl = after_open.find('\n')?;
    let after_nl = &after_open[first_nl + 1..];
    let close = after_nl.rfind("```")?;
    Some(after_nl[..close].trim_end_matches('\n').to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenRow {
    pub token: String,
    pub uses_allowed: Option<u32>,
    pub pending: u32,
    pub completed: u32,
    pub expiration: Option<String>,
}

/// Best-effort parser for `token list`. Tuwunel emits one bullet per token:
///   ``- `TOKEN` --- Token used N times. Expires after M uses or in X days (YYYY-MM-DD HH:MM:SS).``
/// Expiration wording varies: `Expires after M uses`, `or in X days (TS)`, or
/// `Does not expire`. We extract what's there and leave the rest None.
pub fn list_tokens(body: &str) -> Option<Vec<TokenRow>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('`') else {
            continue;
        };
        let Some(end) = rest.find('`') else {
            continue;
        };
        let token = rest[..end].trim().to_string();
        if token.is_empty() {
            continue;
        }
        let tail = &rest[end + 1..];

        let completed = extract_u32(tail, "used ", " time").unwrap_or(0);
        let uses_allowed = extract_u32(tail, "after ", " use");
        let expiration = tail.rfind('(').zip(tail.rfind(')')).and_then(|(a, b)| {
            if a < b {
                Some(tail[a + 1..b].trim().to_string())
            } else {
                None
            }
        });

        out.push(TokenRow {
            token,
            uses_allowed,
            pending: 0,
            completed,
            expiration,
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Pull a u32 from `haystack` sitting between `before` and `after`.
fn extract_u32(haystack: &str, before: &str, after: &str) -> Option<u32> {
    let start = haystack.find(before)? + before.len();
    let rest = &haystack[start..];
    let end = rest.find(after)?;
    rest[..end].trim().parse().ok()
}

/// Extract the first fenced (```…```) payload from a reply body.
fn fenced(s: &str) -> Option<String> {
    let open = s.find("```")?;
    let after_open = &s[open + 3..];
    let first_nl = after_open.find('\n')?;
    let after_nl = &after_open[first_nl + 1..];
    let close = after_nl.rfind("```")?;
    Some(after_nl[..close].to_string())
}

/// Best-effort parser for `media get-file-info` output. Extracts `key: value`
/// pairs from a fenced or plaintext body. Returns None if nothing matched.
pub fn media_file_info(body: &str) -> Option<Vec<(String, String)>> {
    let inner = fenced(body).unwrap_or_else(|| body.to_string());
    let mut out = Vec::new();
    for line in inner.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() || v.is_empty() || k.contains(' ') && k.len() > 40 {
            continue;
        }
        out.push((k.to_string(), v.to_string()));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub display_name: Option<String>,
    pub last_seen_ip: Option<String>,
    pub last_seen_ts: Option<String>,
}

/// Parse `query users list-devices-metadata` output. The body is a Rust
/// `Debug` print of a `Vec<Device>` inside a ```rs fence, like:
/// ```
/// [
///     Device {
///         device_id: "abc",
///         display_name: Some(
///             "foo",
///         ),
///         last_seen_ip: Some("1.2.3.4"),
///         last_seen_ts: Some(2026-04-14T14:15:44.915),
///     },
///     ...
/// ]
/// ```
pub fn list_devices(body: &str) -> Option<Vec<DeviceRow>> {
    let inner = fenced(body)?;
    // Collapse multi-line `Some(\n  value,\n)` into `Some(value,)` to make
    // line-oriented parsing tractable.
    let collapsed = collapse_some_blocks(&inner);
    let mut out = Vec::new();
    let mut cur: Option<DeviceRow> = None;
    for line in collapsed.lines() {
        let line = line.trim();
        if line.starts_with("Device {") {
            cur = Some(DeviceRow {
                device_id: String::new(),
                display_name: None,
                last_seen_ip: None,
                last_seen_ts: None,
            });
            continue;
        }
        if line == "}," || line == "}" {
            if let Some(d) = cur.take() {
                if !d.device_id.is_empty() {
                    out.push(d);
                }
            }
            continue;
        }
        let Some(d) = cur.as_mut() else { continue };
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_end_matches(',').trim();
        let value = parse_debug_value(v);
        match k {
            "device_id" => d.device_id = value.unwrap_or_default(),
            "display_name" => d.display_name = value,
            "last_seen_ip" => d.last_seen_ip = value,
            "last_seen_ts" => d.last_seen_ts = value,
            _ => {}
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Fold `Some(\n    X,\n)` into a single line `Some(X)`, preserving everything
/// else. Used to simplify Rust-Debug parsing.
fn collapse_some_blocks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '(' {
            // Look ahead: if the next non-space char is a newline, consume
            // through the matching close-paren and flatten.
            let mut lookahead = String::new();
            let mut depth = 1;
            let mut saw_newline = false;
            for nc in chars.by_ref() {
                if nc == '\n' {
                    saw_newline = true;
                    continue;
                }
                if nc == '(' {
                    depth += 1;
                } else if nc == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                lookahead.push(nc);
            }
            out.push('(');
            if saw_newline {
                // Flatten: strip surrounding whitespace on each line.
                let flat: String = lookahead
                    .split('\n')
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join("");
                out.push_str(flat.trim_end_matches(','));
            } else {
                out.push_str(&lookahead);
            }
            out.push(')');
        } else {
            out.push(c);
        }
    }
    out
}

/// Handle Rust Debug value forms: `None`, `Some(X)`, quoted strings, bare
/// dates, numbers. Returns the inner value as a string or None.
fn parse_debug_value(v: &str) -> Option<String> {
    let v = v.trim().trim_end_matches(',').trim();
    if v == "None" || v.is_empty() {
        return None;
    }
    let v = v.strip_prefix("Some(").unwrap_or(v);
    let v = v.strip_suffix(')').unwrap_or(v);
    let v = v.trim().trim_end_matches(',').trim();
    if v.is_empty() {
        return None;
    }
    if let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(inner.to_string());
    }
    Some(v.to_string())
}

/// Rooms alias listing. Format:
/// ```
/// Aliases:
/// - `!roomid:server` -> #alias:server
/// - `!roomid:server` -> #alias2:server
/// ```
/// Returns (room_id, alias) pairs.
#[allow(dead_code)]
pub fn list_aliases(body: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('`') else {
            continue;
        };
        let Some(close) = rest.find('`') else {
            continue;
        };
        let room_id = rest[..close].trim().to_string();
        let tail = &rest[close + 1..];
        let Some(arrow) = tail.find("->") else {
            continue;
        };
        let alias = tail[arrow + 2..].trim().to_string();
        if !room_id.is_empty() && !alias.is_empty() {
            out.push((room_id, alias));
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// `rooms alias list <ROOM_ID>` body:
/// ```
/// Aliases for !roomid:server:
/// - #alias:server
/// - #alias2:server
/// ```
/// Returns the list of aliases (strings).
pub fn aliases_for_room(body: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) else {
            continue;
        };
        let rest = rest.trim().trim_matches('`');
        if rest.starts_with('#') && rest.contains(':') {
            out.push(rest.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// `Alias resolves to !roomid:server` → `!roomid:server`.
pub fn alias_resolves_to(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("Alias resolves to "))
        .map(|s| s.trim().to_string())
}

/// `Room topic:\n```\n…\n``` ` → topic text. Also tolerates a body with just
/// the topic or a "no topic" message.
pub fn room_topic(body: &str) -> Option<String> {
    if body.trim().eq_ignore_ascii_case("no topic") {
        return None;
    }
    fenced(body)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `rooms directory list` output: a bullet list of room IDs.
pub fn list_published_rooms(body: &str) -> Option<Vec<String>> {
    if body.trim().to_ascii_lowercase().starts_with("no rooms") {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .unwrap_or(line);
        let rest = rest.trim().trim_matches('`');
        if rest.starts_with('!') && rest.contains(':') {
            out.push(rest.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// `users list-users` header is `Found N local user account(s):`. Returns N,
/// falling back to counting mxid-shaped lines.
pub fn count_users(body: &str) -> usize {
    for line in body.lines() {
        if let Some(rest) = line.trim().strip_prefix("Found ") {
            if let Some(num) = rest.split_whitespace().next() {
                if let Ok(n) = num.parse::<usize>() {
                    return n;
                }
            }
        }
    }
    body.lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with('@') && t.contains(':')
        })
        .count()
}

/// `server memory-usage` body sections: `Services:\n...\nDatabase:\n...\nAllocator:\n...`.
/// Returns the `key: value` pairs parsed from the Database section (e.g.
/// "Memory buffers" → "3.21 MiB"). Returns an empty map if the section is
/// missing.
pub fn memory_database_section(body: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Some(start) = body.find("Database:") else {
        return out;
    };
    let after = &body[start + "Database:".len()..];
    let end = after.find("\nAllocator:").unwrap_or(after.len());
    for line in after[..end].lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        if !k.is_empty() && !v.is_empty() {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

/// `server list-features` output lines like `✅ foo [enabled]` / `❌ foo [disabled]`.
pub fn list_features(body: &str) -> Option<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let (enabled, rest) = if let Some(r) = line.strip_prefix("✅") {
            (true, r.trim())
        } else if let Some(r) = line.strip_prefix("❌") {
            (false, r.trim())
        } else {
            continue;
        };
        let name = rest
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !name.is_empty() {
            out.push((name, enabled));
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Rust-Debug array of strings inside a ```rs fence, like:
/// ```rs
/// [
///     "!room:server",
///     "!room2:server",
/// ]
/// ```
/// Used by `query users get-shared-rooms`, `query users list-devices`, etc.
pub fn debug_string_array(body: &str) -> Option<Vec<String>> {
    let inner = fenced(body)?;
    let mut out = Vec::new();
    for line in inner.lines() {
        let line = line.trim().trim_end_matches(',').trim();
        if let Some(inner) = line.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            out.push(inner.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// `true` / `false` bare reply for `rooms exists`.
#[allow(dead_code)]
pub fn bool_reply(body: &str) -> Option<bool> {
    match body.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `rooms info list-joined-members` body header line:
/// `N Members in Room !roomid:server`. Ignored by `list_joined_members` which
/// only cares about mxids. No extra parser needed.
///
/// `Rooms @mxid Joined (N):\n```\n!room\tMembers: N\tName: X\n...\n````
pub fn list_joined_rooms(body: &str) -> Option<Vec<JoinedRoom>> {
    if !body.trim_start().starts_with("Rooms ") {
        return None;
    }
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if !line.starts_with('!') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let room_id = parts[0].trim().to_string();
        let members = parts[1]
            .trim()
            .strip_prefix("Members:")
            .unwrap_or("0")
            .trim()
            .parse()
            .unwrap_or(0);
        let name = parts[2]
            .trim()
            .strip_prefix("Name:")
            .unwrap_or("")
            .trim()
            .to_string();
        out.push(JoinedRoom {
            room_id,
            members,
            name,
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
