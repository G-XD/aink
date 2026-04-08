//! Resume command generation for supported AI coding tools.

use std::path::Path;

use crate::collector::source::SourceKind;

/// Build the CLI command to resume a session, or `None` if the source doesn't support it.
pub fn resume_command(source: SourceKind, path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    match source {
        SourceKind::Claude => Some(format!("claude --resume {stem}")),
        SourceKind::Codex => {
            // Codex file stems look like `rollout-<timestamp>-<uuid>`.
            // Extract the trailing UUID (36 chars: 8-4-4-4-12).
            if stem.len() >= 36 {
                let uuid = &stem[stem.len() - 36..];
                if is_uuid_shaped(uuid) {
                    return Some(format!("codex resume {uuid}"));
                }
            }
            None
        }
        SourceKind::Cursor | SourceKind::Kiro => None,
    }
}

fn is_uuid_shaped(s: &str) -> bool {
    // xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    if s.len() != 36 {
        return false;
    }
    s.bytes().enumerate().all(|(i, b)| match i {
        8 | 13 | 18 | 23 => b == b'-',
        _ => b.is_ascii_hexdigit(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn claude_resume() {
        let path = PathBuf::from("/home/user/.claude/projects/myproj/abc-def-123.jsonl");
        let cmd = resume_command(SourceKind::Claude, &path);
        assert_eq!(cmd, Some("claude --resume abc-def-123".to_string()));
    }

    #[test]
    fn codex_resume() {
        let path = PathBuf::from(
            "/home/user/.codex/sessions/2025/01/01/rollout-1234567890-a1b2c3d4-e5f6-7890-abcd-ef1234567890.jsonl",
        );
        let cmd = resume_command(SourceKind::Codex, &path);
        assert_eq!(
            cmd,
            Some("codex resume a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string())
        );
    }

    #[test]
    fn cursor_returns_none() {
        let path = PathBuf::from("/some/path/session.jsonl");
        assert_eq!(resume_command(SourceKind::Cursor, &path), None);
    }

    #[test]
    fn kiro_returns_none() {
        let path = PathBuf::from("/some/path/session.jsonl");
        assert_eq!(resume_command(SourceKind::Kiro, &path), None);
    }
}
