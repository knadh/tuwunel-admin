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
