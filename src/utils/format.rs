//! Shared formatting helpers used across multiple UI components.

/// Format a token count into a compact human-readable string (e.g. "1.2M", "45.3k", "999").
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Format a duration in milliseconds to a compact string (e.g. "2h 30m", "5m 12s", "8s").
pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{}h {:02}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {:02}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Get the local timezone offset from UTC in seconds.
fn utc_offset_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let epoch_secs_libc = epoch_secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };

    #[cfg(unix)]
    {
        unsafe { libc::localtime_r(&epoch_secs_libc, &mut tm) };
        tm.tm_gmtoff as i64
    }

    #[cfg(windows)]
    {
        // localtime_s(tm_dest, source_time) — Windows uses reversed argument order.
        if unsafe { libc::localtime_s(&mut tm, &epoch_secs_libc) } != 0 {
            return 0;
        }
        // Windows tm has no tm_gmtoff; derive offset from local time components.
        let days = days_from_civil(tm.tm_year + 1900, (tm.tm_mon + 1) as u32, tm.tm_mday as u32);
        let local_epoch = days as i64 * 86400
            + tm.tm_hour as i64 * 3600
            + tm.tm_min as i64 * 60
            + tm.tm_sec as i64;
        local_epoch - epoch_secs
    }
}

/// Convert (year, month, day) to days since Unix epoch (inverse of `civil_from_days`).
fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u32;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

/// Convert UTC timestamp components to local time.
fn utc_to_local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> (i32, u32, u32, u32, u32) {
    let offset = utc_offset_secs();
    let days = days_from_civil(y, mo, d);
    let total_secs = days as i64 * 86400 + h as i64 * 3600 + mi as i64 * 60 + offset;
    let local_days = total_secs.div_euclid(86400) as i32;
    let rem = total_secs.rem_euclid(86400) as u32;
    let (ly, lm, ld) = civil_from_days(local_days);
    (ly, lm as u32, ld as u32, rem / 3600, (rem % 3600) / 60)
}

/// Parse an ISO 8601 timestamp and return `(y, mo, d, h, mi)` in local time.
///
/// If the timestamp ends with `'Z'` (UTC), it is converted to local time.
/// Otherwise (e.g. Cursor local timestamps), it is returned as-is.
pub fn parse_local_datetime(ts: &str) -> Option<(i32, u32, u32, u32, u32)> {
    if ts.len() < 16 {
        return None;
    }
    let y: i32 = ts[0..4].parse().ok()?;
    let mo: u32 = ts[5..7].parse().ok()?;
    let d: u32 = ts[8..10].parse().ok()?;
    let h: u32 = ts[11..13].parse().ok()?;
    let mi: u32 = ts[14..16].parse().ok()?;
    if ts.ends_with('Z') {
        Some(utc_to_local(y, mo, d, h, mi))
    } else {
        Some((y, mo, d, h, mi))
    }
}

/// Convert an ISO 8601 timestamp to a local `"YYYY-MM-DD"` date string.
///
/// UTC timestamps (ending with `'Z'`) are converted; others are kept as-is.
pub fn ts_to_local_date(ts: &str) -> Option<String> {
    let (y, m, d, _, _) = parse_local_datetime(ts)?;
    Some(format!("{:04}-{:02}-{:02}", y, m, d))
}

/// Format an ISO 8601 timestamp into a compact relative or date string for table display.
///
/// UTC timestamps (ending with `'Z'`) are converted to local time; others are kept as-is.
/// - Today: "HH:MM"
/// - This year: "MM-DD"
/// - Older: "YY-MM-DD"
pub fn format_last_active(ts: Option<&str>) -> String {
    let Some(ts) = ts else {
        return "—".to_string();
    };
    let Some((y, mo, d, h, mi)) = parse_local_datetime(ts) else {
        return ts.to_string();
    };

    let now = now_ymd();

    if (y, mo, d) == now {
        format!("{:02}:{:02}", h, mi)
    } else if y == now.0 {
        format!("{:02}-{:02}", mo, d)
    } else {
        format!("{:02}-{:02}-{:02}", y % 100, mo, d)
    }
}

/// Return `(year, month, day)` in local timezone for N days before today.
pub fn days_ago_ymd(n: u32) -> (i32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let local_secs = secs + utc_offset_secs();
    let days = (local_secs.div_euclid(86400)) as i32 - n as i32;
    let (y, m, d) = civil_from_days(days);
    (y, m as u32, d as u32)
}

/// Format a `(year, month, day)` tuple as `"YYYY-MM-DD"`.
pub fn format_date_ymd(y: i32, m: u32, d: u32) -> String {
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Parse a `"YYYY-MM-DD"` string into `(year, month, day)`, returning `None` on invalid input.
pub fn parse_date_ymd(s: &str) -> Option<(i32, u32, u32)> {
    if s.len() != 10 {
        return None;
    }
    let y: i32 = s.get(0..4)?.parse().ok()?;
    let m: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// Return `(year, month, day)` for the current local date.
pub fn now_ymd() -> (i32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let local_secs = secs + utc_offset_secs();
    let days = local_secs.div_euclid(86400) as i32;
    let (y, mo, d) = civil_from_days(days);
    (y, mo as u32, d as u32)
}

pub fn civil_from_days(z: i32) -> (i32, i32, i32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as i32, d as i32)
}

/// Format an ISO 8601 timestamp for conversation list: "HH:MM" or "MM-DD HH:MM".
///
/// UTC timestamps (ending with `'Z'`) are converted to local time; others are kept as-is.
pub fn format_conversation_time(ts: Option<&str>) -> String {
    let Some(ts) = ts else {
        return "—".to_string();
    };
    let Some((y, mo, d, h, mi)) = parse_local_datetime(ts) else {
        return ts.to_string();
    };
    let now = now_ymd();
    if (y, mo, d) == now {
        format!("{:02}:{:02}", h, mi)
    } else {
        format!("{:02}-{:02} {:02}:{:02}", mo, d, h, mi)
    }
}

/// Format a USD cost to a compact string (e.g. "$1.23", "$10.5", "$100").
pub fn format_cost(usd: f64) -> String {
    let usd = usd.max(0.0);
    if usd >= 100.0 {
        format!("${:.0}", usd)
    } else if usd >= 10.0 {
        format!("${:.1}", usd)
    } else {
        format!("${:.2}", usd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_ago_ymd_today() {
        let today = now_ymd();
        assert_eq!(days_ago_ymd(0), today);
    }

    #[test]
    fn test_days_ago_ymd_7() {
        let (y, m, d) = days_ago_ymd(7);
        let today = now_ymd();
        let today_str = format!("{:04}-{:02}-{:02}", today.0, today.1, today.2);
        let ago_str = format!("{:04}-{:02}-{:02}", y, m, d);
        assert!(ago_str < today_str);
    }

    #[test]
    fn test_format_date_ymd() {
        assert_eq!(format_date_ymd(2025, 1, 15), "2025-01-15");
        assert_eq!(format_date_ymd(2026, 12, 3), "2026-12-03");
    }

    #[test]
    fn test_parse_date_ymd_valid() {
        assert_eq!(parse_date_ymd("2025-01-15"), Some((2025, 1, 15)));
        assert_eq!(parse_date_ymd("2026-12-03"), Some((2026, 12, 3)));
    }

    #[test]
    fn test_parse_date_ymd_invalid() {
        assert_eq!(parse_date_ymd("not-a-date"), None);
        assert_eq!(parse_date_ymd("2025-13-01"), None);
        assert_eq!(parse_date_ymd("2025-00-01"), None);
        assert_eq!(parse_date_ymd("2025-01-32"), None);
        assert_eq!(parse_date_ymd("2025-01-00"), None);
        assert_eq!(parse_date_ymd("short"), None);
    }
}
