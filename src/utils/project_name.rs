//! Decode encoded project/session folder names to readable display names.
//!
//! Claude Code (and similar tools) store sessions in folders like:
//! - `-home-user-projects-myproject` → `myproject`
//! - `-mnt-c-Users-name-Projects-app` → `app`
//!
//! Reusable for Codex, OpenCode, or any backend that uses similar path encoding.

use std::path::Path;

const PREFIXES: &[&str] = &["-home-", "-mnt-c-Users-", "-mnt-c-users-", "-Users-"];
const SKIP_DIRS: &[&str] = &[
    "projects",
    "code",
    "repos",
    "src",
    "dev",
    "work",
    "documents",
];

/// Try to strip a known prefix from the encoded folder name.
///
/// Handles static prefixes from `PREFIXES` plus a dynamic pattern for
/// Windows native drive letters: `-X-Users-` where X is a single ASCII letter.
fn strip_known_prefix(name: &str) -> &str {
    for prefix in PREFIXES {
        if let Some(rest) = case_insensitive_strip(name, prefix) {
            return rest;
        }
    }
    // Windows native: `-C-Users-`, `-D-Users-`, etc.
    if name.len() >= 10
        && name.starts_with('-')
        && name.as_bytes()[1].is_ascii_alphabetic()
        && name[2..].starts_with("-Users-")
    {
        return &name[9..];
    }
    name
}

/// Case-insensitive prefix strip that is safe on UTF-8 boundaries.
fn case_insensitive_strip<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = s.get(..prefix.len())?;
    if rest.eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Convert an encoded folder name to a readable project name.
///
/// # Examples
/// - `-home-user-projects-myproject` → `myproject`
/// - `-mnt-c-Users-name-Projects-app` → `app`
/// - `-C-Users-name-Projects-app` → `app`
pub fn project_display_name(folder_name: &str) -> String {
    let name = strip_known_prefix(folder_name);
    let parts: Vec<&str> = name.split('-').collect();
    let mut meaningful_parts: Vec<&str> = Vec::new();
    let mut found_project = false;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 && !found_project {
            let remaining_lower: Vec<String> =
                parts[i + 1..].iter().map(|p| p.to_lowercase()).collect();
            let has_skip = SKIP_DIRS
                .iter()
                .any(|d| remaining_lower.iter().any(|p| p == *d));
            if has_skip {
                continue;
            }
        }
        if SKIP_DIRS.iter().any(|d| part.eq_ignore_ascii_case(d)) {
            found_project = true;
            continue;
        }
        meaningful_parts.push(part);
        found_project = true;
    }
    if !meaningful_parts.is_empty() {
        return meaningful_parts.join("-");
    }
    for part in parts.iter().rev() {
        if !part.is_empty() {
            return (*part).to_string();
        }
    }
    folder_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_display_name_home() {
        assert_eq!(
            project_display_name("-home-user-projects-myproject"),
            "myproject"
        );
    }

    #[test]
    fn test_project_display_name_windows() {
        assert_eq!(
            project_display_name("-mnt-c-Users-name-Projects-app"),
            "app"
        );
    }

    #[test]
    fn test_project_display_name_fallback() {
        assert_eq!(project_display_name("plain-name"), "plain-name");
    }

    #[test]
    fn test_project_display_name_windows_native() {
        assert_eq!(project_display_name("-C-Users-name-Projects-app"), "app");
        assert_eq!(
            project_display_name("-D-Users-dev-code-frontend"),
            "frontend"
        );
    }
}

/// Session display name from a transcript file path and data.
///
/// Priority: `project_name` (set by each source) → `slug` → path-based fallback.
pub fn session_display_name(
    path: &Path,
    data: &crate::collector::transcript::TranscriptData,
    max_len: usize,
) -> String {
    let name = if let Some(ref s) = data.project_name
        && !s.is_empty()
    {
        s.clone()
    } else if let Some(ref s) = data.slug
        && !s.is_empty()
    {
        s.clone()
    } else {
        let folder_name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .unwrap_or("?");
        project_display_name(folder_name)
    };

    truncate_display_name(&name, max_len)
}

fn truncate_display_name(name: &str, max_len: usize) -> String {
    if name.len() > max_len {
        let n = max_len.saturating_sub(1);
        let end = name
            .char_indices()
            .take(n)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}…", &name[..end])
    } else {
        name.to_string()
    }
}
