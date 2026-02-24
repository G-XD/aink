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

/// Format an ISO 8601 timestamp into a compact relative or date string for table display.
///
/// - Today: "HH:MM"
/// - Yesterday: "Yesterday"
/// - This year: "MM-DD"
/// - Older: "YY-MM-DD"
pub fn format_last_active(ts: Option<&str>) -> String {
    let Some(ts) = ts else {
        return "—".to_string();
    };
    if ts.len() < 16 {
        return ts.to_string();
    }

    let y: i32 = ts[0..4].parse().unwrap_or(0);
    let mo: u32 = ts[5..7].parse().unwrap_or(0);
    let d: u32 = ts[8..10].parse().unwrap_or(0);
    let h: u32 = ts[11..13].parse().unwrap_or(0);
    let mi: u32 = ts[14..16].parse().unwrap_or(0);

    let now = now_ymd();

    if (y, mo, d) == now {
        format!("{:02}:{:02}", h, mi)
    } else if is_yesterday((y, mo, d), now) {
        "Yest.".to_string()
    } else if y == now.0 {
        format!("{:02}-{:02}", mo, d)
    } else {
        format!("{:02}-{:02}-{:02}", y % 100, mo, d)
    }
}

fn now_ymd() -> (i32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = (secs / 86400) as i32;
    let (y, mo, d) = civil_from_days(days);
    (y, mo as u32, d as u32)
}

fn is_yesterday(date: (i32, u32, u32), today: (i32, u32, u32)) -> bool {
    let d1 = days_from_civil(date.0, date.1, date.2);
    let d2 = days_from_civil(today.0, today.1, today.2);
    d2 - d1 == 1
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u32;
    let m = m as i32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u32;
    era * 146097 + doe as i32 - 719468
}

fn civil_from_days(z: i32) -> (i32, i32, i32) {
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
