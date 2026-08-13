//! Mock listing backend for the browser screen (E5). The initial path returns
//! the exact fixture listing; every other directory gets a deterministic
//! synthesized listing so navigation feels real until the real `VfsBackend`
//! (E4) lands. `mk-ui` never blocks on network — this is the whole point of
//! developing against fixtures first.

/// The ancestor path shown in the `..` row's label (e.g. `/export/media`).
pub fn parent_of(cwd: &str) -> String {
    let t = cwd.trim_end_matches('/');
    match t.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => t[..i].to_string(),
        None => "/".to_string(),
    }
}

/// Smallest path segment (used as the `..` label's basename).
pub fn base_name(cwd: &str) -> &str {
    let t = cwd.trim_end_matches('/');
    if t.is_empty() {
        return "/";
    }
    t.rsplit('/').next().unwrap_or("/")
}

/// Mock lazily-fetched codec/dimensions for a file (E6-S2). Returns `None`
/// for non-media, mirroring "omitted (not blank-labelled) when unavailable".
pub fn codec_for(name: &str) -> Option<String> {
    if name.ends_with(".mkv") || name.ends_with(".mp4") || name.ends_with(".ts") {
        if name.len().is_multiple_of(2) {
            Some("hevc 3840×2160 · dts-hd".into())
        } else {
            Some("hevc 1920×1080 · aac".into())
        }
    } else {
        None
    }
}

/// Mock "decoded frame": a deterministic gradient derived from the file name.
/// A real decode pipeline replaces this when it lands.
pub fn thumb_css(name: &str) -> String {
    let hue: u32 = name.bytes().fold(0x9E37u32, |a, b| {
        a.wrapping_mul(31).wrapping_add(u32::from(b))
    }) % 360;
    format!(
        "linear-gradient(135deg, hsl({hue} 32% 16%), hsl({} 38% 26%))",
        (hue + 70) % 360
    )
}

/// Deterministic mock SHA-256 (real hashing is E7-S4; this only matches the
/// display shape).
pub fn sha256_for(name: &str) -> String {
    let mut acc: u64 = 0xCBF2_9CE4_8422_2325;
    for b in name.bytes() {
        acc ^= u64::from(b);
        acc = acc.wrapping_mul(0x0100_0000_01B3);
    }
    format!("sha256 {acc:016x}…")
}

/// Directory component of a path (`/Downloads/file.mkv` -> `/Downloads`).
pub fn dir_of(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".into(),
        Some(i) => path[..i].into(),
        None => ".".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_paths() {
        assert_eq!(parent_of("/export/media/films"), "/export/media");
        assert_eq!(parent_of("/"), "/");
        assert_eq!(base_name("/export/media/films"), "films");
    }
}
