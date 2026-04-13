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
