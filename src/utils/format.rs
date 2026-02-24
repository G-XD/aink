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
