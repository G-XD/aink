//! Source-agnostic transcript data models.
//!
//! These types are shared across all transcript sources (Claude, Codex, Cursor, …).
//! Source-specific parsing lives in sibling modules (e.g. `claude.rs`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::source::SourceKind;

/// Per-model token usage breakdown.
#[derive(Clone, Debug, Default)]
pub struct ModelStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub tool_call_count: u64,
}

/// Aggregated data from a transcript session.
#[derive(Clone)]
#[allow(dead_code)]
pub struct TranscriptData {
    pub source: SourceKind,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub models: HashSet<String>,
    pub tool_call_total: u64,
    pub tool_call_by_type: HashMap<String, u64>,
    pub files_touched: Vec<String>,
    pub first_user_message: Option<String>,
    pub per_model: HashMap<String, ModelStats>,
    pub duration_ms: u64,
    pub turn_count: u64,
    pub user_message_count: u64,
    pub assistant_message_count: u64,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub agent_version: Option<String>,
    pub git_branch: Option<String>,
    pub slug: Option<String>,
    pub estimated_cost_usd: f64,
}

/// Structured detail for a single tool call within a conversation turn.
#[derive(Clone, Debug)]
pub struct ToolCallDetail {
    pub name: String,
    /// One-line summary of the most relevant input parameter (e.g. file path, command).
    pub summary: String,
}

/// One turn in the conversation (user or assistant message).
#[derive(Clone, Debug)]
pub struct ConversationTurn {
    pub role: ConversationRole,
    pub content: String,
    pub tool_calls: Vec<ToolCallDetail>,
    /// ISO 8601 timestamp when the message was sent (e.g. from `createdAt` / `timestamp`).
    pub created_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationRole {
    User,
    Assistant,
}

/// Type alias for the common transcript list used across components.
pub type SessionList = Vec<(PathBuf, TranscriptData)>;
