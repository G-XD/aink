//! Transcript source abstraction.
//!
//! Each AI coding tool (Claude Code, Codex, Cursor, …) stores transcripts
//! in its own format and directory layout.  The [`TranscriptSource`] trait
//! provides a uniform interface for discovery, parsing, and cost estimation
//! so the rest of the application remains source-agnostic.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::transcript::{ConversationTurn, TranscriptData};
use crate::config::SourceConfig;

/// Identifies which AI coding tool produced a transcript.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    #[default]
    Claude,
    Cursor,
    Codex,
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => write!(f, "Claude"),
            Self::Cursor => write!(f, "Cursor"),
            Self::Codex => write!(f, "Codex"),
        }
    }
}

/// A transcript source knows how to discover, parse, and interpret transcripts
/// from a specific AI coding tool.
pub trait TranscriptSource: Send + Sync {
    fn kind(&self) -> SourceKind;

    /// Discover all transcript file paths under the given root directory.
    /// Returns paths sorted by modification time (newest first).
    fn discover_paths(&self, root: &Path) -> Vec<PathBuf>;

    /// Parse a single transcript file into aggregated statistics.
    #[allow(dead_code)]
    fn parse_transcript(&self, path: &Path) -> color_eyre::Result<TranscriptData>;

    /// Parse a transcript file into the full conversation history.
    fn parse_conversation(&self, path: &Path) -> color_eyre::Result<Vec<ConversationTurn>>;

    /// Load and aggregate all transcripts from discovered paths.
    /// Failed parses are logged and skipped.
    fn load_transcripts(&self, paths: &[PathBuf]) -> Vec<(PathBuf, TranscriptData)>;
}

/// Build the appropriate [`TranscriptSource`] implementation for a given config.
pub fn create_source(config: &SourceConfig) -> Box<dyn TranscriptSource> {
    match config.kind {
        SourceKind::Claude => Box::new(super::claude::ClaudeSource),
        SourceKind::Cursor => Box::new(super::cursor::CursorSource),
        SourceKind::Codex => Box::new(super::codex::CodexSource),
    }
}
