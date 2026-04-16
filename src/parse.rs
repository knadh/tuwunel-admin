//! Best-effort parsers for tuwunel admin-bot reply bodies. On shape mismatch
//! they return `None`/empty and the caller falls back to raw markdown.

/// `users list-users`: mxids from the fenced block under the `Found N ...` header.
pub fn list_users(body: &str) -> Option<Vec<String>> {
    if !body.trim_start().starts_with("Found ") {
        return None;
    }
    let v: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('@') && l.contains(':'))
        .map(str::to_string)
        .collect();
    (!v.is_empty()).then_some(v)
}

/// `users last-active`: `YYYY-MM-DDTHH:MM:SS.mmm localpart` lines.
/// Returns (localpart, timestamp) pairs.
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
        if !localpart.is_empty() {
            out.push((localpart.to_string(), ts.to_string()));
        }
    }
    (!out.is_empty()).then_some(out)
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

/// `rooms list`: `!room_id\tMembers: N\tName: X` lines, fields in any order.
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
        for p in parts.map(str::trim) {
            if let Some(rest) = p.strip_prefix("Members:") {
                members = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = p.strip_prefix("Name:") {
                name = rest.trim().to_string();
            }
        }
        if name == room_id {
            name.clear();
        }
        out.push(RoomRow {
            room_id,
            name,
            members,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// `rooms info list-joined-members`: pulls the first `@localpart:server`
/// token off each line, ignoring display-name suffixes and HTML wrapping.
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
    (!mxids.is_empty()).then_some(mxids)
}

/// `federation incoming-federation`: room_ids with federation enabled.
pub fn list_federated_rooms(body: &str) -> Option<Vec<String>> {
    list_bang_ids(body)
}

/// `rooms moderation list-banned-rooms`: banned room_ids.
pub fn list_banned_rooms(body: &str) -> Option<Vec<String>> {
    list_bang_ids(body)
}

/// `Appservices (N): id1, id2, ...`. Returns an empty Vec for N=0;
/// None only if the `Appservices` header is missing.
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

/// `appservices show-config` YAML body, i.e. the payload inside the yaml fence.
pub fn appservice_config_yaml(body: &str) -> Option<String> {
    fenced(body).map(|s| s.trim_end_matches('\n').to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenRow {
    pub token: String,
    pub uses_allowed: Option<u32>,
    pub pending: u32,
    pub completed: u32,
    pub expiration: Option<String>,
}

/// `token list`: one bullet per token:
///   ``- `TOKEN` --- Token used N times. Expires after M uses or in X days (YYYY-MM-DD HH:MM:SS).``
/// Expiration wording varies (`after M uses`, `in X days (TS)`, `Does not expire`);
/// missing pieces become None.
pub fn list_tokens(body: &str) -> Option<Vec<TokenRow>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(rest) = bullet(line.trim()).and_then(|r| r.strip_prefix('`')) else {
            continue;
        };
        let Some(end) = rest.find('`') else { continue };
        let token = rest[..end].trim().to_string();
        if token.is_empty() {
            continue;
        }
        let tail = &rest[end + 1..];

        let completed = extract_u32(tail, "used ", " time").unwrap_or(0);
        let uses_allowed = extract_u32(tail, "after ", " use");
        let expiration = tail
            .rfind('(')
            .zip(tail.rfind(')'))
            .and_then(|(a, b)| (a < b).then(|| tail[a + 1..b].trim().to_string()));

        out.push(TokenRow {
            token,
            uses_allowed,
            pending: 0,
            completed,
            expiration,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// Parse a u32 from the slice between the first `before` and the next `after`.
fn extract_u32(haystack: &str, before: &str, after: &str) -> Option<u32> {
    let start = haystack.find(before)? + before.len();
    let rest = &haystack[start..];
    let end = rest.find(after)?;
    rest[..end].trim().parse().ok()
}

/// ASCII case-insensitive `str::starts_with`. Unlike `to_ascii_lowercase`,
/// this doesn't allocate a lowercased copy of the whole string.
pub fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Extract the first fenced (```…```) payload from a reply body.
pub fn fenced(s: &str) -> Option<&str> {
    let open = s.find("```")?;
    let after_open = &s[open + 3..];
    let first_nl = after_open.find('\n')?;
    let after_nl = &after_open[first_nl + 1..];
    let close = after_nl.rfind("```")?;
    Some(&after_nl[..close])
}

/// Strip `- ` or `* ` off a markdown bullet line.
fn bullet(line: &str) -> Option<&str> {
    line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))
}

/// First whitespace-delimited token from each `!`-prefixed line.
/// Shared by `list-banned-rooms`, `incoming-federation`, etc.
fn list_bang_ids(body: &str) -> Option<Vec<String>> {
    let ids: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('!'))
        .map(|l| l.split_whitespace().next().unwrap_or(l).to_string())
        .collect();
    (!ids.is_empty()).then_some(ids)
}

/// `media get-file-info`: `key: value` pairs from a fenced or plain body.
pub fn media_file_info(body: &str) -> Option<Vec<(String, String)>> {
    let inner = fenced(body).unwrap_or(body);
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
    (!out.is_empty()).then_some(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub display_name: Option<String>,
    pub last_seen_ip: Option<String>,
    pub last_seen_ts: Option<String>,
}

/// `query users list-devices-metadata`: a `Vec<Device>` Rust-Debug print
/// inside a ```rs fence, with `Some(...)` values broken across lines.
pub fn list_devices(body: &str) -> Option<Vec<DeviceRow>> {
    let inner = fenced(body)?;
    // Flatten `Some(\n  value,\n)` to `Some(value,)` for line-oriented parsing.
    let collapsed = collapse_some_blocks(inner);
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
    (!out.is_empty()).then_some(out)
}

/// Fold `(\n    X,\n)` into `(X)`, preserving nesting and non-multiline parens.
fn collapse_some_blocks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '(' {
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

/// Unwrap one Rust-Debug value: `None`, `Some(X)`, `"string"`, or a bare token.
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

/// `rooms alias list`: bullets of ``- `!room` -> #alias``. Returns (room, alias) pairs.
#[allow(dead_code)]
pub fn list_aliases(body: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(rest) = bullet(line.trim()).and_then(|r| r.strip_prefix('`')) else {
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
    (!out.is_empty()).then_some(out)
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
        let Some(rest) = bullet(line.trim()) else {
            continue;
        };
        let rest = rest.trim().trim_matches('`');
        if rest.starts_with('#') && rest.contains(':') {
            out.push(rest.to_string());
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `Alias resolves to !roomid:server` → `!roomid:server`.
pub fn alias_resolves_to(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("Alias resolves to "))
        .map(|s| s.trim().to_string())
}

/// Topic text from `rooms info view-room-topic`, or None for "no topic".
pub fn room_topic(body: &str) -> Option<String> {
    if body.trim().eq_ignore_ascii_case("no topic") {
        return None;
    }
    let t = fenced(body)?.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// `rooms directory list`: bullet list of published room IDs.
pub fn list_published_rooms(body: &str) -> Option<Vec<String>> {
    if starts_with_ci(body.trim(), "no rooms") {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let rest = bullet(line).unwrap_or(line).trim().trim_matches('`');
        if rest.starts_with('!') && rest.contains(':') {
            out.push(rest.to_string());
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `server memory-usage`: `Services:` / `Database:` / `Allocator:` sections,
/// each a block of `key: value` lines. Non-`key:value` lines are dropped.
pub fn memory_sections(body: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut current: Option<String> = Some("General".to_string());
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        // Bare `label:` (no value) is a section header.
        if let Some(label) = line.strip_suffix(':') {
            if !label.is_empty() && !label.contains(':') {
                current = Some(label.trim().to_string());
                continue;
            }
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() || v.is_empty() {
            continue;
        }
        let section = current.clone().unwrap_or_else(|| "General".to_string());
        if let Some(entry) = out.iter_mut().find(|(s, _)| s == &section) {
            entry.1.push((k.to_string(), v.to_string()));
        } else {
            out.push((section, vec![(k.to_string(), v.to_string())]));
        }
    }
    out.retain(|(_, rows)| !rows.is_empty());
    out
}

/// `server show-config`: rows of a `| name | value |` markdown table.
/// Values are returned verbatim (may contain markdown).
pub fn config_table(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if !line.starts_with('|') || !line.ends_with('|') {
            continue;
        }
        let inner = &line[1..line.len() - 1];
        let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
        if cells.len() < 2 {
            continue;
        }
        let name = cells[0];
        let value = cells[1];
        if name.is_empty() || name.eq_ignore_ascii_case("name") {
            continue;
        }
        // Markdown alignment row (`:---` / `---`).
        if name.chars().all(|c| matches!(c, ':' | '-' | ' ')) {
            continue;
        }
        out.push((name.to_string(), value.to_string()));
    }
    out
}

/// Extract `N` from the `Found N local user account(s):` header, or count mxids.
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
            (true, r)
        } else if let Some(r) = line.strip_prefix("❌") {
            (false, r)
        } else {
            continue;
        };
        if let Some(name) = rest.split_whitespace().next() {
            out.push((name.to_string(), enabled));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Rust-Debug `["a", "b", ...]` inside a ```rs fence. Used by
/// `query users get-shared-rooms`, `query users list-devices`, etc.
pub fn debug_string_array(body: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for line in fenced(body)?.lines() {
        let line = line.trim().trim_end_matches(',').trim();
        if let Some(inner) = line.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            out.push(inner.to_string());
        }
    }
    (!out.is_empty()).then_some(out)
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

/// `users list-joined-rooms`: `!room\tMembers: N\tName: X` lines under the
/// `Rooms @mxid Joined (N):` header.
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
            room_id: parts[0].trim().to_string(),
            members,
            name,
        });
    }
    (!out.is_empty()).then_some(out)
}
