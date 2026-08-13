//! User-pinned files and folders.

use serde::{Deserialize, Serialize};

use crate::host::EntryKind;
use crate::recent::normalize_path;

/// One saved file or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub host_id: String,
    pub path: String,
    #[serde(default = "default_kind")]
    pub kind: EntryKind,
    /// Unix seconds when the bookmark was added.
    pub added_at: i64,
}

fn default_kind() -> EntryKind {
    EntryKind::Dir
}

pub fn contains(list: &[Bookmark], host_id: &str, path: &str) -> bool {
    let path = normalize_path(path);
    list.iter().any(|b| b.host_id == host_id && b.path == path)
}

/// Insert if missing. Returns true when a new bookmark was added.
pub fn add(
    list: &mut Vec<Bookmark>,
    host_id: String,
    path: String,
    kind: EntryKind,
    now: i64,
) -> bool {
    let host_id = host_id.trim().to_string();
    let path = normalize_path(&path);
    if host_id.is_empty() || path.is_empty() {
        return false;
    }
    if contains(list, &host_id, &path) {
        return false;
    }
    list.insert(
        0,
        Bookmark {
            host_id,
            path,
            kind,
            added_at: now,
        },
    );
    true
}

/// Remove matching bookmark. Returns true when something was dropped.
pub fn remove(list: &mut Vec<Bookmark>, host_id: &str, path: &str) -> bool {
    let path = normalize_path(path);
    let before = list.len();
    list.retain(|b| !(b.host_id == host_id && b.path == path));
    list.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_dedups_and_remove() {
        let mut list = Vec::new();
        assert!(add(
            &mut list,
            "h".into(),
            "/a/b/".into(),
            EntryKind::Dir,
            1
        ));
        assert!(!add(
            &mut list,
            "h".into(),
            "/a/b".into(),
            EntryKind::Dir,
            2
        ));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, "/a/b");
        assert!(remove(&mut list, "h", "/a/b/"));
        assert!(list.is_empty());
    }

    #[test]
    fn add_rejects_empty() {
        let mut list = Vec::new();
        assert!(!add(&mut list, "".into(), "/a".into(), EntryKind::File, 1));
        assert!(!add(&mut list, "h".into(), "  ".into(), EntryKind::File, 1));
    }
}
