//! Formatting helpers that turn raw values into the exact strings the design
//! handoff specifies. These are covered by tests asserting the literal mockup
//! strings (`18.4G`, `8.4M/s`, `19m 04s`, `-rw-r--r--`, `2026-05-02 11:20:44`).

use chrono::{Local, TimeZone, Utc};

use crate::EntryKind;

/// Human size with one decimal when fractional, none when integral
/// (`84K`, `4.1K`, `128M`, `18.4G`, `1.9T`). 1024-based units, no spaces.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    if bytes < 1024 {
        return format!("{bytes}B");
    }
    let mut v = bytes as f64;
    let mut unit = 0usize;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}{}", v, UNITS[unit])
    } else {
        format!("{:.1}{}", v, UNITS[unit])
    }
}

/// Exact byte count with thousands separators, e.g. `25,884,266,496 B`.
pub fn format_size_exact(bytes: u64) -> String {
    let digits = bytes.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 2);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out.push_str(" B");
    out
}

/// Transfer rate, e.g. `8.4M/s`, `12.1M/s`, `20M/s`. 1024-based bytes.
pub fn format_rate(bps: f64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = bps;
    let mut unit = 0usize;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}{}/s", v, UNITS[unit])
    } else {
        format!("{:.1}{}/s", v, UNITS[unit])
    }
}

/// ETA like `19m 04s` (or `2h 05m` beyond the hour).
pub fn format_eta(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m {s:02}s")
    }
}

/// Octal permission string, e.g. `0644`.
pub fn format_mode_octal(mode: u32) -> String {
    format!("{mode:04o}")
}

/// Symbolic permission string including the type char, e.g. `-rw-r--r--`.
pub fn format_mode_symbolic(mode: u32, kind: EntryKind) -> String {
    let mut out = String::with_capacity(10);
    out.push(kind.type_char());
    let bits = [
        0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
    ];
    let chars = ['r', 'w', 'x', 'r', 'w', 'x', 'r', 'w', 'x'];
    for (bit, c) in bits.iter().zip(chars) {
        out.push(if mode & bit != 0 { c } else { '-' });
    }
    out
}

/// Combined inspector form, e.g. `0644 -rw-r--r--`.
pub fn format_mode_full(mode: u32, kind: EntryKind) -> String {
    format!(
        "{} {}",
        format_mode_octal(mode),
        format_mode_symbolic(mode, kind)
    )
}

/// Wall-clock timestamp in the local timezone, e.g. `2026-05-02 11:20:44`.
pub fn format_mtime(secs: i64) -> String {
    format_timestamp(secs, false)
}

/// Deterministic UTC variant used by fixtures and tests so the exact mockup
/// timestamps hold on every machine.
pub fn format_mtime_utc(secs: i64) -> String {
    format_timestamp(secs, true)
}

fn format_timestamp(secs: i64, utc: bool) -> String {
    if utc {
        match Utc.timestamp_opt(secs, 0).single() {
            Some(t) => t.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "—".to_string(),
        }
    } else {
        match Local.timestamp_opt(secs, 0).single() {
            Some(t) => t.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "—".to_string(),
        }
    }
}

/// Relative time like `4 min ago`, `2 d ago`, `8 mo ago`.
pub fn format_ago(secs: u64) -> String {
    const MIN: u64 = 60;
    const HOUR: u64 = 3600;
    const DAY: u64 = 86_400;
    const MONTH: u64 = 30 * DAY;
    if secs < MIN {
        format!("{secs}s ago")
    } else if secs < HOUR {
        format!("{} min ago", secs / MIN)
    } else if secs < DAY {
        format!("{}h ago", secs / HOUR)
    } else if secs < MONTH {
        format!("{} d ago", secs / DAY)
    } else {
        format!("{} mo ago", secs / MONTH)
    }
}

/// Duration like `4h 12m` (mount age, session age).
pub fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h}h {m:02}m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_mockups() {
        let g = |x: f64| (x * 1024.0f64.powi(3)).round() as u64;
        assert_eq!(format_size(84 * 1024), "84K");
        assert_eq!(format_size(61 * 1024), "61K");
        assert_eq!(format_size(812 * 1024), "812K");
        assert_eq!(format_size((4.1f64 * 1024.0).round() as u64), "4.1K");
        assert_eq!(format_size(128 * 1024 * 1024), "128M");
        assert_eq!(format_size(g(18.4)), "18.4G");
        assert_eq!(format_size(g(9.8)), "9.8G");
        assert_eq!(format_size(g(38.0)), "38G");
        assert_eq!(format_size(g(24.1)), "24.1G");
        assert_eq!(format_size(g(4.2) * 1024), "4.2T");
        assert_eq!(format_size(g(1.9) * 1024), "1.9T");
        // The mockup's exact byte figure for BladeRunner must round-trip.
        assert_eq!(format_size(25_884_266_496), "24.1G");
    }

    #[test]
    fn exact_bytes() {
        assert_eq!(format_size_exact(25_884_266_496), "25,884,266,496 B");
        assert_eq!(format_size_exact(0), "0 B");
        assert_eq!(format_size_exact(1_000_000), "1,000,000 B");
    }

    #[test]
    fn rates_match_mockups() {
        assert_eq!(format_rate(8.4 * 1024.0 * 1024.0), "8.4M/s");
        assert_eq!(format_rate(12.1 * 1024.0 * 1024.0), "12.1M/s");
        assert_eq!(format_rate(20.0 * 1024.0 * 1024.0), "20M/s");
        assert_eq!(format_rate(512.0 * 1024.0), "512K/s");
    }

    #[test]
    fn etas_match_mockups() {
        assert_eq!(format_eta(1144), "19m 04s");
        assert_eq!(format_eta(60), "1m 00s");
        assert_eq!(format_eta(7_500), "2h 05m");
    }

    #[test]
    fn modes_match_mockups() {
        assert_eq!(format_mode_full(0o644, EntryKind::File), "0644 -rw-r--r--");
        assert_eq!(format_mode_full(0o600, EntryKind::File), "0600 -rw-------");
        assert_eq!(format_mode_full(0o755, EntryKind::Dir), "0755 drwxr-xr-x");
        assert_eq!(
            format_mode_full(0o777, EntryKind::Symlink),
            "0777 lrwxrwxrwx"
        );
        assert_eq!(format_mode_symbolic(0o644, EntryKind::File), "-rw-r--r--");
    }

    #[test]
    fn timestamps() {
        // 2026-05-02 11:20:44 UTC.
        let secs = Utc
            .with_ymd_and_hms(2026, 5, 2, 11, 20, 44)
            .unwrap()
            .timestamp();
        assert_eq!(format_mtime_utc(secs), "2026-05-02 11:20:44");
        // The local variant produces the same `YYYY-MM-DD HH:MM:SS` shape in
        // whatever timezone the test machine is in.
        let local = format_mtime(secs);
        assert_eq!(local.len(), 19);
        assert!(local.starts_with("2026-"));
    }

    #[test]
    fn ago_and_duration() {
        assert_eq!(format_ago(240), "4 min ago");
        assert_eq!(format_ago(2 * 86_400), "2 d ago");
        assert_eq!(format_ago(8 * 30 * 86_400), "8 mo ago");
        assert_eq!(format_duration(4 * 3600 + 12 * 60), "4h 12m");
    }
}
