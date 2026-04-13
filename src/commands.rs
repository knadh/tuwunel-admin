//! Catalog of admin commands.
//!
//! Each `Cmd` maps to a tuwunel admin room command string assembled
//! using form fields. To add a command, append a `Cmd` entry to `ALL`.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Cmd {
    pub module: &'static str,
    pub action: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
    pub fields: &'static [Field],

    /// Template for the command string. `{name}` is substituted with the field value.
    /// `[--flag {name}]` is included only if `name` (a checkbox) is checked. If the field
    /// is text, `[--flag {name}]` is included only if the value is non-empty.
    pub template: &'static str,
    pub danger: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub placeholder: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    Text,
    Password,
    Textarea,
    Checkbox,
    Number,
}

pub const MODULES: &[(&str, &str)] = &[
    ("users", "Users"),
    ("rooms", "Rooms"),
    ("federation", "Federation"),
    ("appservice", "App services"),
    ("media", "Media"),
    ("tokens", "Registration tokens"),
    ("server", "Server"),
    ("diagnostics", "Diagnostics"),
];

pub const ALL: &[Cmd] = &[
    // Users.

    Cmd {
        module: "users",
        action: "list-users",
        label: "List all users",
        desc: "List every local user on the server.",
        fields: &[],
        template: "users list-users",
        danger: false,
    },
    Cmd {
        module: "users",
        action: "last-active",
        label: "Last active users",
        desc: "List users ordered by last activity.",
        fields: &[Field { name: "limit", label: "Limit", kind: FieldKind::Number, placeholder: "50", required: false }],
        template: "users last-active [--limit {limit}]",
        danger: false,
    },
    Cmd {
        module: "users",
        action: "create",
        label: "Create user",
        desc: "Create a new local user account.",
        fields: &[
            Field { name: "username", label: "Username (localpart)", kind: FieldKind::Text, placeholder: "alice", required: true },
            Field { name: "password", label: "Password", kind: FieldKind::Password, placeholder: "", required: true },
        ],
        template: "users create-user {username} {password}",
        danger: false,
    },
    Cmd {
        module: "users",
        action: "reset-password",
        label: "Reset password",
        desc: "Force-reset a user's password.",
        fields: &[
            Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true },
            Field { name: "password", label: "New password", kind: FieldKind::Password, placeholder: "", required: true },
        ],
        template: "users reset-password {user_id} {password}",
        danger: true,
    },
    Cmd {
        module: "users",
        action: "deactivate",
        label: "Deactivate user",
        desc: "Deactivate a user account.",
        fields: &[
            Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true },
            Field { name: "no_leave_rooms", label: "Keep user in rooms (don't leave)", kind: FieldKind::Checkbox, placeholder: "", required: false },
        ],
        template: "users deactivate {user_id} [--no-leave-rooms no_leave_rooms]",
        danger: true,
    },
    Cmd {
        module: "users",
        action: "make-admin",
        label: "Grant server admin",
        desc: "Promote a user to server admin.",
        fields: &[Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true }],
        template: "users make-user-admin {user_id}",
        danger: true,
    },
    Cmd {
        module: "users",
        action: "list-joined-rooms",
        label: "List user's joined rooms",
        desc: "Show rooms a user is currently joined to.",
        fields: &[Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true }],
        template: "users list-joined-rooms {user_id}",
        danger: false,
    },
    Cmd {
        module: "users",
        action: "force-join",
        label: "Force-join user to room",
        desc: "Make a user join a room.",
        fields: &[
            Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true },
            Field { name: "room", label: "Room ID or alias", kind: FieldKind::Text, placeholder: "!abc:server or #room:server", required: true },
        ],
        template: "users force-join-room {user_id} {room}",
        danger: true,
    },
    Cmd {
        module: "users",
        action: "force-leave",
        label: "Force-leave user from room",
        desc: "Make a user leave a room.",
        fields: &[
            Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true },
            Field { name: "room_id", label: "Room ID", kind: FieldKind::Text, placeholder: "!abc:server", required: true },
        ],
        template: "users force-leave-room {user_id} {room_id}",
        danger: true,
    },
    Cmd {
        module: "users",
        action: "redact-event",
        label: "Redact event",
        desc: "Force-redact a message event.",
        fields: &[Field { name: "event_id", label: "Event ID", kind: FieldKind::Text, placeholder: "$abc...", required: true }],
        template: "users redact-event {event_id}",
        danger: true,
    },

    // Rooms
    Cmd {
        module: "rooms",
        action: "list-rooms",
        label: "List rooms",
        desc: "List all known rooms (paginated by tuwunel).",
        fields: &[Field { name: "page", label: "Page", kind: FieldKind::Number, placeholder: "1", required: false }],
        template: "rooms list {page}",
        danger: false,
    },
    Cmd {
        module: "rooms",
        action: "list-banned",
        label: "List banned rooms",
        desc: "Show rooms that are currently banned.",
        fields: &[],
        template: "rooms moderation list-banned-rooms",
        danger: false,
    },
    Cmd {
        module: "rooms",
        action: "ban-room",
        label: "Ban room",
        desc: "Ban a room server-wide.",
        fields: &[Field { name: "room", label: "Room ID or alias", kind: FieldKind::Text, placeholder: "!abc:server or #room:server", required: true }],
        template: "rooms moderation ban-room {room}",
        danger: true,
    },
    Cmd {
        module: "rooms",
        action: "unban-room",
        label: "Unban room",
        desc: "Lift a server-wide room ban.",
        fields: &[Field { name: "room", label: "Room ID or alias", kind: FieldKind::Text, placeholder: "!abc:server or #room:server", required: true }],
        template: "rooms moderation unban-room {room}",
        danger: false,
    },
    Cmd {
        module: "rooms",
        action: "list-members",
        label: "List members in room",
        desc: "Show members of a room.",
        fields: &[
            Field { name: "room_id", label: "Room ID", kind: FieldKind::Text, placeholder: "!abc:server", required: true },
            Field { name: "local_only", label: "Local users only", kind: FieldKind::Checkbox, placeholder: "", required: false },
        ],
        template: "rooms info list-joined-members {room_id} [--local-only local_only]",
        danger: false,
    },
    Cmd {
        module: "rooms",
        action: "delete",
        label: "Delete room",
        desc: "Delete a room from the server.",
        fields: &[
            Field { name: "room_id", label: "Room ID", kind: FieldKind::Text, placeholder: "!abc:server", required: true },
            Field { name: "force", label: "Force", kind: FieldKind::Checkbox, placeholder: "", required: false },
        ],
        template: "rooms delete {room_id} [--force force]",
        danger: true,
    },

    // Federation
    Cmd {
        module: "federation",
        action: "incoming",
        label: "Incoming federation",
        desc: "List rooms receiving incoming federation PDUs.",
        fields: &[],
        template: "federation incoming-federation",
        danger: false,
    },
    Cmd {
        module: "federation",
        action: "disable-room",
        label: "Disable federation for room",
        desc: "",
        fields: &[Field { name: "room_id", label: "Room ID", kind: FieldKind::Text, placeholder: "!abc:server", required: true }],
        template: "federation disable-room {room_id}",
        danger: true,
    },
    Cmd {
        module: "federation",
        action: "enable-room",
        label: "Enable federation for room",
        desc: "",
        fields: &[Field { name: "room_id", label: "Room ID", kind: FieldKind::Text, placeholder: "!abc:server", required: true }],
        template: "federation enable-room {room_id}",
        danger: false,
    },
    Cmd {
        module: "federation",
        action: "remote-user-rooms",
        label: "Rooms shared with remote user",
        desc: "List rooms this server shares with a remote user.",
        fields: &[Field { name: "user_id", label: "User ID", kind: FieldKind::Text, placeholder: "@bob:other.server", required: true }],
        template: "federation remote-user-in-rooms {user_id}",
        danger: false,
    },

    // Appservice
    Cmd {
        module: "appservice",
        action: "list",
        label: "List appservices",
        desc: "List registered application services.",
        fields: &[],
        template: "appservices list",
        danger: false,
    },
    Cmd {
        module: "appservice",
        action: "show",
        label: "Show appservice config",
        desc: "",
        fields: &[Field { name: "id", label: "Appservice ID", kind: FieldKind::Text, placeholder: "", required: true }],
        template: "appservices show-config {id}",
        danger: false,
    },
    Cmd {
        module: "appservice",
        action: "register",
        label: "Register appservice",
        desc: "Register an appservice. Paste the registration YAML in the body.",
        fields: &[Field { name: "yaml", label: "Registration YAML", kind: FieldKind::Textarea, placeholder: "id: example\nurl: http://...\nas_token: ...\nhs_token: ...\nsender_localpart: ...", required: true }],
        template: "appservices register\n```\n{yaml}\n```",
        danger: true,
    },
    Cmd {
        module: "appservice",
        action: "unregister",
        label: "Unregister appservice",
        desc: "",
        fields: &[Field { name: "id", label: "Appservice ID", kind: FieldKind::Text, placeholder: "", required: true }],
        template: "appservices unregister {id}",
        danger: true,
    },

    // Media
    Cmd {
        module: "media",
        action: "delete",
        label: "Delete media",
        desc: "Delete a single media item by MXC URL or event ID.",
        fields: &[
            Field { name: "mxc", label: "MXC URL (optional)", kind: FieldKind::Text, placeholder: "mxc://server/abc", required: false },
            Field { name: "event_id", label: "Event ID (optional)", kind: FieldKind::Text, placeholder: "$abc...", required: false },
        ],
        template: "media delete [--mxc {mxc}] [--event-id {event_id}]",
        danger: true,
    },
    Cmd {
        module: "media",
        action: "delete-past-remote",
        label: "Delete old remote media",
        desc: "Purge remote media older/newer than a duration (e.g. '30d').",
        fields: &[
            Field { name: "duration", label: "Duration (e.g. 30d, 6h)", kind: FieldKind::Text, placeholder: "30d", required: true },
            Field { name: "after", label: "Delete media NEWER than duration (default is older)", kind: FieldKind::Checkbox, placeholder: "", required: false },
        ],
        template: "media delete-past-remote-media {duration} [--after after]",
        danger: true,
    },
    Cmd {
        module: "media",
        action: "delete-from-user",
        label: "Delete all media from user",
        desc: "",
        fields: &[Field { name: "user", label: "User ID", kind: FieldKind::Text, placeholder: "@alice:server", required: true }],
        template: "media delete-all-from-user {user}",
        danger: true,
    },
    Cmd {
        module: "media",
        action: "file-info",
        label: "Media file info",
        desc: "",
        fields: &[Field { name: "mxc", label: "MXC URL", kind: FieldKind::Text, placeholder: "mxc://server/abc", required: true }],
        template: "media get-file-info {mxc}",
        danger: false,
    },

    // Tokens
    Cmd {
        module: "tokens",
        action: "list",
        label: "List registration tokens",
        desc: "",
        fields: &[],
        template: "token list",
        danger: false,
    },
    Cmd {
        module: "tokens",
        action: "issue",
        label: "Issue registration token",
        desc: "",
        fields: &[
            Field { name: "max_uses", label: "Max uses", kind: FieldKind::Number, placeholder: "1", required: false },
            Field { name: "max_age", label: "Max age (e.g. 7d)", kind: FieldKind::Text, placeholder: "7d", required: false },
            Field { name: "once", label: "One-time use", kind: FieldKind::Checkbox, placeholder: "", required: false },
        ],
        template: "token issue [--max-uses {max_uses}] [--max-age {max_age}] [--once once]",
        danger: false,
    },
    Cmd {
        module: "tokens",
        action: "revoke",
        label: "Revoke token",
        desc: "",
        fields: &[Field { name: "token", label: "Token", kind: FieldKind::Text, placeholder: "", required: true }],
        template: "token revoke {token}",
        danger: true,
    },

    // Server
    Cmd {
        module: "server",
        action: "uptime",
        label: "Uptime",
        desc: "",
        fields: &[],
        template: "server uptime",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "show-config",
        label: "Show current config",
        desc: "Boot-time config as loaded. To change values, edit the config/compose file and reload.",
        fields: &[],
        template: "server show-config",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "reload-config",
        label: "Reload config file",
        desc: "Re-read the on-disk config after edits.",
        fields: &[Field { name: "path", label: "Path (optional)", kind: FieldKind::Text, placeholder: "", required: false }],
        template: "server reload-config {path}",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "memory",
        label: "Memory usage",
        desc: "",
        fields: &[],
        template: "server memory-usage",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "clear-caches",
        label: "Clear caches",
        desc: "",
        fields: &[],
        template: "server clear-caches",
        danger: true,
    },
    Cmd {
        module: "server",
        action: "backup",
        label: "Backup database",
        desc: "Trigger an online database backup.",
        fields: &[],
        template: "server backup-database",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "list-backups",
        label: "List backups",
        desc: "",
        fields: &[],
        template: "server list-backups",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "admin-notice",
        label: "Send admin notice",
        desc: "Broadcast a notice to the admin room.",
        fields: &[Field { name: "message", label: "Message", kind: FieldKind::Textarea, placeholder: "", required: true }],
        template: "server admin-notice {message}",
        danger: false,
    },
    Cmd {
        module: "server",
        action: "reload-mods",
        label: "Hot-reload server",
        desc: "",
        fields: &[],
        template: "server reload-mods",
        danger: true,
    },
    Cmd {
        module: "server",
        action: "restart",
        label: "Restart server",
        desc: "",
        fields: &[Field { name: "force", label: "Force", kind: FieldKind::Checkbox, placeholder: "", required: false }],
        template: "server restart [--force force]",
        danger: true,
    },
    Cmd {
        module: "server",
        action: "shutdown",
        label: "Shutdown server",
        desc: "",
        fields: &[],
        template: "server shutdown",
        danger: true,
    },

    // Diagnostics
    Cmd {
        module: "diagnostics",
        action: "ping",
        label: "Ping federation server",
        desc: "",
        fields: &[Field { name: "server", label: "Server name", kind: FieldKind::Text, placeholder: "matrix.org", required: true }],
        template: "debug ping {server}",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "resolve-destination",
        label: "Resolve federation destination",
        desc: "",
        fields: &[Field { name: "server", label: "Server name", kind: FieldKind::Text, placeholder: "matrix.org", required: true }],
        template: "debug resolve-true-destination {server}",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "change-log-level",
        label: "Change log level",
        desc: "",
        fields: &[
            Field { name: "filter", label: "Filter (tracing-subscriber syntax)", kind: FieldKind::Text, placeholder: "info,tuwunel=debug", required: false },
            Field { name: "reset", label: "Reset to default", kind: FieldKind::Checkbox, placeholder: "", required: false },
        ],
        template: "debug change-log-level [--filter {filter}] [--reset reset]",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "db-stats",
        label: "Database stats",
        desc: "",
        fields: &[],
        template: "debug database-stats",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "runtime-metrics",
        label: "Runtime metrics",
        desc: "",
        fields: &[],
        template: "debug runtime-metrics",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "memory-stats",
        label: "Memory stats",
        desc: "",
        fields: &[],
        template: "debug memory-stats",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "trim-memory",
        label: "Trim memory",
        desc: "",
        fields: &[],
        template: "debug trim-memory",
        danger: false,
    },
    Cmd {
        module: "diagnostics",
        action: "raw",
        label: "Raw admin command",
        desc: "Run any admin command. The body is sent as-is to the admin room. Use for debug/query subcommands not exposed above.",
        fields: &[Field { name: "cmd", label: "Command", kind: FieldKind::Textarea, placeholder: "query globals", required: true }],
        template: "{cmd}",
        danger: true,
    },
];

/// Lookup by module + action.
pub fn find(module: &str, action: &str) -> Option<&'static Cmd> {
    ALL.iter()
        .find(|c| c.module == module && c.action == action)
}

/// Commands grouped by module, preserving the order in `ALL`.
pub fn by_module(module: &str) -> Vec<&'static Cmd> {
    ALL.iter().filter(|c| c.module == module).collect()
}

/// Render the command string for a `Cmd` given form values.
///
/// Template rules:
/// - `{name}` is replaced with the field's value (empty string if missing).
/// - `[--flag {name}]` is emitted as `--flag <value>` only if the value is non-empty
///   (for text/number fields) or truthy (for checkboxes).
/// - `[--flag name]` (withoutbraces) treats `name` as a boolean; emits `--flag` if checked.
pub fn render_template(cmd: &Cmd, values: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(cmd.template.len() + 32);
    let tpl = cmd.template;
    let mut i = 0;
    let bytes = tpl.as_bytes();
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'[' {
            // find closing ]
            if let Some(end_rel) = tpl[i + 1..].find(']') {
                let end = i + 1 + end_rel;
                let inner = &tpl[i + 1..end];
                if let Some(rendered) = render_optional(inner, values) {
                    out.push_str(&rendered);
                }
                i = end + 1;
                continue;
            }
        }
        if c == b'{' {
            if let Some(end_rel) = tpl[i + 1..].find('}') {
                let end = i + 1 + end_rel;
                let name = &tpl[i + 1..end];
                if let Some(v) = values.get(name) {
                    out.push_str(v);
                }
                i = end + 1;
                continue;
            }
        }
        out.push(c as char);
        i += 1;
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_optional(inner: &str, values: &HashMap<String, String>) -> Option<String> {
    // Two forms:
    //   "--flag {name}"  => include if values[name] is non-empty; emit `--flag <value>`
    //   "--flag name"    => include if values[name] is truthy (== "on" / "true" / "1");
    //                       emit just `--flag`.

    let trimmed = inner.trim();
    if let Some(open) = trimmed.find('{') {
        let close = trimmed.find('}')?;
        let name = &trimmed[open + 1..close];
        let v = values.get(name).map(|s| s.as_str()).unwrap_or("").trim();
        if v.is_empty() {
            return None;
        }
        let flag = trimmed[..open].trim();
        return Some(format!("{flag} {v}"));
    }
    // Bare name => boolean
    let parts: Vec<&str> = trimmed.rsplitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let name = parts[0];
    let flag = parts[1];
    let v = values.get(name).map(|s| s.as_str()).unwrap_or("");
    if matches!(v, "on" | "true" | "1" | "yes") {
        Some(flag.to_string())
    } else {
        None
    }
}
