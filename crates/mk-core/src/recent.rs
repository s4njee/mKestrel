//! Recently visited folders (host + path).
//!
//! The rail's Recent place lists these, newest first. Persistence keeps the
//! list across launches; the UI records a visit whenever the browser lands
//! on a directory.

use serde::{Deserialize, Serialize};

/// Maximum remembered folders. Older entries drop off the tail.
pub const RECENT_LIMIT: usize = 30;

/// One folder the user has opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentPath {
    pub host_id: String,
    pub path: String,
    /// Unix seconds when this folder was last opened.
    pub visited_at: i64,
}

/// Normalize a remote path for identity: trim, drop a trailing slash except
/// for the root `/`. Empty input stays empty so [`touch`] can reject it.
pub fn normalize_path(path: &str) -> String {
    let t = path.trim();
    if t.is_empty() {
        return String::new();
    }
    if t == "/" {
        return "/".into();
    }
    t.trim_end_matches('/').to_string()
}

/// Last path segment, or `/` for the root.
pub fn folder_name(path: &str) -> &str {
    let t = path.trim_end_matches('/');
    if t.is_empty() {
        return "/";
    }
    t.rsplit('/').next().unwrap_or("/")
}

/// Insert or bump `(host_id, path)` to the front of `list`. No-op when either
/// side is empty. Caps the list at [`RECENT_LIMIT`].
pub fn touch(list: &mut Vec<RecentPath>, host_id: String, path: String, now: i64) {
    let host_id = host_id.trim().to_string();
    let path = normalize_path(&path);
    if host_id.is_empty() || path.is_empty() {
        return;
    }
    list.retain(|r| !(r.host_id == host_id && r.path == path));
    list.insert(
        0,
        RecentPath {
            host_id,
            path,
            visited_at: now,
        },
    );
    list.truncate(RECENT_LIMIT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_trailing_slash() {
        assert_eq!(normalize_path("/export/media/"), "/export/media");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("  /a/b  "), "/a/b");
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn folder_name_is_last_segment() {
        assert_eq!(folder_name("/export/media/films"), "films");
        assert_eq!(folder_name("/export/media/films/"), "films");
        assert_eq!(folder_name("/"), "/");
    }

    #[test]
    fn touch_inserts_front_and_dedups() {
        let mut list = Vec::new();
        touch(&mut list, "h1".into(), "/a".into(), 1);
        touch(&mut list, "h1".into(), "/b".into(), 2);
        touch(&mut list, "h1".into(), "/a/".into(), 3);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].path, "/a");
        assert_eq!(list[0].visited_at, 3);
        assert_eq!(list[1].path, "/b");
    }

    #[test]
    fn touch_caps_at_limit() {
        let mut list = Vec::new();
        for i in 0..(RECENT_LIMIT + 5) {
            touch(&mut list, "h".into(), format!("/p{i}"), i as i64);
        }
        assert_eq!(list.len(), RECENT_LIMIT);
        assert_eq!(list[0].path, format!("/p{}", RECENT_LIMIT + 4));
        assert_eq!(list.last().unwrap().path, "/p5");
    }

    #[test]
    fn touch_ignores_empty() {
        let mut list = Vec::new();
        touch(&mut list, "".into(), "/a".into(), 1);
        touch(&mut list, "h".into(), "  ".into(), 1);
        assert!(list.is_empty());
    }
}
