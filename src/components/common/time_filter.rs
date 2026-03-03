//! Time range filter: pure data + logic, no UI coupling.

use crate::utils::format::{days_ago_ymd, format_date_ymd, parse_date_ymd};

/// Time range filter for session list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TimeFilter {
    #[default]
    All,
    Today,
    Last7Days,
    Last30Days,
    Custom { from: String, to: String },
}

fn month_abbrev(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

impl TimeFilter {
    /// Returns `true` for any filter other than `All`.
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::All)
    }

    /// Human-readable label for this filter.
    ///
    /// - `All` → "All time"
    /// - `Today` → "Today"
    /// - `Last7Days` → "Last 7 days"
    /// - `Last30Days` → "Last 30 days"
    /// - `Custom` same year → "Jan 15 ~ Feb 01, 2025"
    /// - `Custom` different years → "2024-12-01 ~ 2025-02-01"
    pub fn label(&self) -> String {
        match self {
            Self::All => "All time".to_string(),
            Self::Today => "Today".to_string(),
            Self::Last7Days => "Last 7 days".to_string(),
            Self::Last30Days => "Last 30 days".to_string(),
            Self::Custom { from, to } => {
                let from_parsed = parse_date_ymd(from);
                let to_parsed = parse_date_ymd(to);
                match (from_parsed, to_parsed) {
                    (Some((fy, fm, fd)), Some((ty, tm, td))) if fy == ty => {
                        format!(
                            "{} {:02} ~ {} {:02}, {}",
                            month_abbrev(fm),
                            fd,
                            month_abbrev(tm),
                            td,
                            fy
                        )
                    }
                    _ => {
                        format!("{} ~ {}", from, to)
                    }
                }
            }
        }
    }

    /// Checks if an ISO 8601 timestamp falls within this filter's range.
    ///
    /// - `All` matches everything (including `None`).
    /// - Other variants return `false` for `None`.
    /// - Compares the date portion (first 10 chars) against `date_range()`.
    pub fn matches(&self, end_time: &Option<String>) -> bool {
        if matches!(self, Self::All) {
            return true;
        }
        let Some(ts) = end_time else {
            return false;
        };
        if ts.len() < 10 {
            return false;
        }
        let date = &ts[..10];
        let (start, end) = self.date_range();
        date >= start.as_str() && date <= end.as_str()
    }

    /// Parse a CLI argument into a `TimeFilter`.
    ///
    /// Accepts: "today", "7d", "30d", "YYYY-MM-DD..YYYY-MM-DD".
    /// Returns `None` for invalid input.
    pub fn from_range_arg(s: &str) -> Option<Self> {
        match s {
            "today" => Some(Self::Today),
            "7d" => Some(Self::Last7Days),
            "30d" => Some(Self::Last30Days),
            other => {
                let parts: Vec<&str> = other.splitn(2, "..").collect();
                if parts.len() != 2 {
                    return None;
                }
                let from_date = parse_date_ymd(parts[0])?;
                let to_date = parse_date_ymd(parts[1])?;
                Some(Self::Custom {
                    from: format_date_ymd(from_date.0, from_date.1, from_date.2),
                    to: format_date_ymd(to_date.0, to_date.1, to_date.2),
                })
            }
        }
    }

    /// Returns `(start_date, end_date)` as `"YYYY-MM-DD"` strings.
    fn date_range(&self) -> (String, String) {
        match self {
            Self::All => ("0000-00-00".to_string(), "9999-12-31".to_string()),
            Self::Today => {
                let (y, m, d) = days_ago_ymd(0);
                let today = format_date_ymd(y, m, d);
                (today.clone(), today)
            }
            Self::Last7Days => {
                let (y, m, d) = days_ago_ymd(6);
                let start = format_date_ymd(y, m, d);
                let (y, m, d) = days_ago_ymd(0);
                let end = format_date_ymd(y, m, d);
                (start, end)
            }
            Self::Last30Days => {
                let (y, m, d) = days_ago_ymd(29);
                let start = format_date_ymd(y, m, d);
                let (y, m, d) = days_ago_ymd(0);
                let end = format_date_ymd(y, m, d);
                (start, end)
            }
            Self::Custom { from, to } => (from.clone(), to.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label() {
        assert_eq!(TimeFilter::All.label(), "All time");
        assert_eq!(TimeFilter::Today.label(), "Today");
        assert_eq!(TimeFilter::Last7Days.label(), "Last 7 days");
        assert_eq!(TimeFilter::Last30Days.label(), "Last 30 days");
        assert_eq!(
            TimeFilter::Custom {
                from: "2025-01-15".into(),
                to: "2025-02-01".into()
            }
            .label(),
            "Jan 15 ~ Feb 01, 2025"
        );
        assert_eq!(
            TimeFilter::Custom {
                from: "2024-12-01".into(),
                to: "2025-02-01".into()
            }
            .label(),
            "2024-12-01 ~ 2025-02-01"
        );
    }

    #[test]
    fn test_all_matches_everything() {
        assert!(TimeFilter::All.matches(&None));
        assert!(TimeFilter::All.matches(&Some("2025-01-01T00:00:00Z".into())));
    }

    #[test]
    fn test_preset_excludes_none() {
        assert!(!TimeFilter::Today.matches(&None));
        assert!(!TimeFilter::Last7Days.matches(&None));
        assert!(!TimeFilter::Last30Days.matches(&None));
    }

    #[test]
    fn test_custom_matches_in_range() {
        let filter = TimeFilter::Custom {
            from: "2025-01-15".into(),
            to: "2025-02-01".into(),
        };
        assert!(filter.matches(&Some("2025-01-15T10:00:00Z".into())));
        assert!(filter.matches(&Some("2025-01-20T00:00:00Z".into())));
        assert!(filter.matches(&Some("2025-02-01T23:59:59Z".into())));
        assert!(!filter.matches(&Some("2025-01-14T23:59:59Z".into())));
        assert!(!filter.matches(&Some("2025-02-02T00:00:00Z".into())));
        assert!(!filter.matches(&None));
    }

    #[test]
    fn test_from_range_arg() {
        assert_eq!(TimeFilter::from_range_arg("today"), Some(TimeFilter::Today));
        assert_eq!(
            TimeFilter::from_range_arg("7d"),
            Some(TimeFilter::Last7Days)
        );
        assert_eq!(
            TimeFilter::from_range_arg("30d"),
            Some(TimeFilter::Last30Days)
        );
        assert_eq!(
            TimeFilter::from_range_arg("2025-01-15..2025-02-01"),
            Some(TimeFilter::Custom {
                from: "2025-01-15".into(),
                to: "2025-02-01".into()
            })
        );
        assert_eq!(TimeFilter::from_range_arg("garbage"), None);
        assert_eq!(TimeFilter::from_range_arg(""), None);
    }

    #[test]
    fn test_is_active() {
        assert!(!TimeFilter::All.is_active());
        assert!(TimeFilter::Today.is_active());
        assert!(TimeFilter::Last7Days.is_active());
        assert!(TimeFilter::Last30Days.is_active());
        assert!(TimeFilter::Custom {
            from: "2025-01-15".into(),
            to: "2025-02-01".into()
        }
        .is_active());
    }
}
